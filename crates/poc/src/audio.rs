//! Sound: frx7 decodes each source to samples and brings them to the mix's rate and channels,
//! we mix them, and frx7 encodes the mix.

use crate::encode::registries;
use crate::model::Audio;
use frx7_audio::Resampler;
use frx7_base::Codec;
use frx7_codec::{AudioEncoderSetup, AudioSource};
use frx7_export::SoundFile;
use std::io::{Read, Write};
use std::path::Path;

/// Samples a second of the mix. It has two channels.
pub const RATE: u32 = 48_000;

/// The highest the finished mix may reach, between its samples as well as on them: 1 dB under
/// full scale.
pub const CEILING: f32 = 0.891_250_9;

/// What is known of a mix once it is made.
pub struct Mixed {
    /// The highest peak of the mix as made, between the samples as well as on them.
    pub peak: f32,
    /// By how many decibels the whole mix was lowered to keep its peak under the ceiling.
    pub lowered: f32,
}

/// How many values are worked out between two samples to find a peak there.
const BETWEEN: usize = 4;
/// How many samples on each side of a place take part in the value there.
const REACH: usize = 6;

/// The weights that give the value `phase` quarters of the way from one sample to the next:
/// a windowed sinc over the samples around it.
fn weights(phase: usize) -> [f32; 2 * REACH] {
    let offset = phase as f64 / BETWEEN as f64;
    std::array::from_fn(|tap| {
        // The distance of this sample from the place, in samples.
        let x = tap as f64 - (REACH as f64 - 1.0) - offset;
        let sinc = if x.abs() < 1e-12 {
            1.0
        } else {
            (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
        };
        let window = 0.5 + 0.5 * (std::f64::consts::PI * x / REACH as f64).cos();
        (sinc * window) as f32
    })
}

/// Follows the highest peak of a sound given piece by piece, left and right in turn, looking
/// between the samples too and across the joins of two pieces. Only places near a sample that
/// is already loud can hold the peak, so only those are worked out: loud against the loudest
/// sample met so far.
pub struct Peak {
    on_samples: f32,
    peak: f32,
    phases: Vec<[f32; 2 * REACH]>,
    /// The samples still needed, left and right in turn.
    kept: Vec<f32>,
    /// Which frame of the whole sound `kept` begins with.
    first: usize,
    /// The next frame to look around.
    next: usize,
}

impl Default for Peak {
    fn default() -> Peak {
        Peak::new()
    }
}

impl Peak {
    pub fn new() -> Peak {
        Peak {
            on_samples: 0.0,
            peak: 0.0,
            phases: (1..BETWEEN).map(weights).collect(),
            kept: Vec::new(),
            first: 0,
            next: REACH,
        }
    }

    /// Takes the next piece: a whole number of frames.
    pub fn add(&mut self, piece: &[f32]) {
        self.on_samples = piece.iter().fold(self.on_samples, |peak, sample| peak.max(sample.abs()));
        self.peak = self.peak.max(self.on_samples);
        self.kept.extend_from_slice(piece);
        let near = self.on_samples * 0.5;
        let end = self.first + self.kept.len() / 2;
        // A frame is looked around once the frames after it that take part are there.
        while self.next + REACH < end {
            let frame = self.next - self.first;
            for channel in 0..2 {
                let at = |frame: usize| self.kept[frame * 2 + channel];
                if at(frame).abs() < near && at(frame + 1).abs() < near {
                    continue;
                }
                for weights in &self.phases {
                    let mut value = 0.0_f32;
                    for (tap, weight) in weights.iter().enumerate() {
                        value += at(frame + tap + 1 - REACH) * weight;
                    }
                    self.peak = self.peak.max(value.abs());
                }
            }
            self.next += 1;
        }
        // The frames before the next one's reach are done with.
        let done = (self.next + 1).saturating_sub(REACH).saturating_sub(self.first);
        self.kept.drain(..done * 2);
        self.first += done;
    }

    /// The highest peak of everything taken.
    pub fn highest(&self) -> f32 {
        self.peak
    }
}

/// The highest peak of samples that hold left and right in turn, looking between the samples
/// too.
pub fn true_peak(samples: &[f32]) -> f32 {
    let mut peak = Peak::new();
    peak.add(&samples[..samples.len() / 2 * 2]);
    peak.highest()
}

/// Keeps a mix under the ceiling: one that goes over is lowered as a whole, by the least that
/// brings its highest peak to the ceiling. Returns the peak as it was and the decibels taken off.
pub fn limit(samples: &mut [f32]) -> (f32, f32) {
    let peak = true_peak(samples);
    let Some(gain) = lowering(peak) else { return (peak, 0.0) };
    for sample in samples.iter_mut() {
        *sample *= gain;
    }
    (peak, -20.0 * gain.log10())
}

/// What a mix with this highest peak is multiplied by to come down to the ceiling; nothing for
/// a mix that stays under it.
fn lowering(peak: f32) -> Option<f32> {
    (peak > CEILING).then(|| CEILING / peak)
}

/// How many frames of the mix are made at a time: four seconds.
pub const STRETCH: usize = RATE as usize * 4;
/// How many frames of a source are read at a time.
const BLOCK: usize = 8192;

/// One sound of the timeline while it is mixed.
struct Playing<'a> {
    sound: &'a Audio,
    /// The frame of the mix it begins at, and how many frames of it the timeline plays.
    first: usize,
    wanted: usize,
    /// How many of its frames are mixed.
    done: usize,
    /// The automation point the current sample lies after.
    after: usize,
    decoder: Option<Decoder>,
    over: bool,
}

/// The part of one sound the timeline plays, as samples of the mix's rate and channels.
struct Decoder {
    source: Box<dyn AudioSource>,
    channels: u16,
    resampler: Resampler,
    /// Frames of the source still to read.
    left: u64,
    /// Whether everything the source gives is in `ready`.
    drained: bool,
    /// A block of the source as it comes, and as left and right.
    read: Vec<f32>,
    both: Vec<f32>,
    /// Samples of the mix's form that are made and not yet taken, from `taken` on.
    ready: Vec<f32>,
    taken: usize,
}

impl Decoder {
    /// Opens the sound and moves to where the timeline starts playing it.
    fn start(folder: &Path, audio: &Audio) -> Result<Decoder, String> {
        let path = folder.join(&audio.source);
        let failed = |error: frx7_base::Error| format!("the sound `{}` could not be read: {error}", audio.source.display());
        let (_, codecs) = registries();
        let mut source = codecs.audio_source(&path).map_err(failed)?;
        let (rate, channels) = (source.sample_rate(), source.channels());
        if !matches!(channels, 1 | 2 | 6) {
            return Err(format!(
                "the sound of {} has {channels} channels; one, two and six are mixed",
                path.display()
            ));
        }
        // No move to the very beginning: the source already stands there.
        if audio.media_start > 0.0 {
            source.seek((audio.media_start * f64::from(rate)).round() as u64).map_err(failed)?;
        }
        Ok(Decoder {
            source,
            channels,
            resampler: Resampler::new(rate, RATE, 2).map_err(failed)?,
            // A frame more than the clip lasts, so that rounding leaves none missing.
            left: (audio.duration * f64::from(rate)).ceil() as u64 + 1,
            drained: false,
            read: Vec::new(),
            both: Vec::new(),
            ready: Vec::new(),
            taken: 0,
        })
    }

    /// Reads the next block of the source into `ready`, or what is left inside the resampler
    /// once the source has ended.
    fn more(&mut self) -> Result<(), frx7_base::Error> {
        let block = (self.left.min(BLOCK as u64)) as usize;
        let channels = usize::from(self.channels);
        self.read.resize(block * channels, 0.0);
        let read = if block == 0 { 0 } else { self.source.read(&mut self.read)? };
        self.left = if read < block { 0 } else { self.left - read as u64 };
        if read == 0 {
            self.resampler.finish(&mut self.ready);
            self.drained = true;
            return Ok(());
        }
        let block = &self.read[..read * channels];
        if channels == 2 {
            self.resampler.push(block, &mut self.ready);
        } else {
            self.both.clear();
            frx7_audio::mix(block, self.channels, 2, &mut self.both)?;
            self.resampler.push(&self.both, &mut self.ready);
        }
        Ok(())
    }

    /// The next `frames` frames, left and right in turn; fewer only at the end of the sound.
    fn take(&mut self, frames: usize) -> Result<&[f32], frx7_base::Error> {
        // What was taken before is done with.
        self.ready.drain(..self.taken);
        self.taken = 0;
        while self.ready.len() < frames * 2 && !self.drained {
            self.more()?;
        }
        self.taken = self.ready.len().min(frames * 2) / 2 * 2;
        Ok(&self.ready[..self.taken])
    }
}

impl Playing<'_> {
    /// Adds this sound's part of the stretch that begins at frame `from` of the mix.
    fn add_to(&mut self, stretch: &mut [f32], from: usize, folder: &Path) -> Result<(), String> {
        let until = from + stretch.len() / 2;
        let (begin, end) = ((self.first + self.done).max(from), (self.first + self.wanted).min(until));
        if self.over || begin >= end {
            return Ok(());
        }
        let decoder = match &mut self.decoder {
            Some(decoder) => decoder,
            decoder => decoder.insert(Decoder::start(folder, self.sound)?),
        };
        let path = folder.join(&self.sound.source);
        let samples = decoder
            .take(end - begin)
            .map_err(|error| format!("the sound of {} could not be decoded: {error}", path.display()))?;
        let (points, volume) = (&self.sound.automation, self.sound.volume);
        for (index, frame) in samples.as_chunks::<2>().0.iter().enumerate() {
            let t = (self.done + index) as f64 / f64::from(RATE);
            while self.after + 1 < points.len() && points[self.after + 1].0 <= t {
                self.after += 1;
            }
            let lane = match points.get(self.after) {
                None => 1.0,
                Some((at, value)) if t <= *at || self.after + 1 == points.len() => *value,
                Some((at, value)) => {
                    let (next_at, next_value) = points[self.after + 1];
                    value + (next_value - value) * (t - at) / (next_at - at)
                }
            };
            let gain = (volume * lane) as f32;
            let place = (begin - from + index) * 2;
            stretch[place] += frame[0] * gain;
            stretch[place + 1] += frame[1] * gain;
        }
        let read = samples.len() / 2;
        self.done += read;
        // The sound ended before its clip did (its source is shorter), or its clip is over.
        if read < end - begin || self.done == self.wanted {
            self.end();
        }
        Ok(())
    }

    /// Lets go of the decoder.
    fn end(&mut self) {
        self.over = true;
        self.decoder = None;
    }
}

/// Mixes every sound of the timeline into `seconds` of sound and encodes it to AAC of
/// `bitrate` kbit/s in `output`, a few seconds at a time. See
/// `docs/spec/native-render-pipeline.md`, section 7.
pub fn render(folder: &Path, sounds: &[Audio], seconds: f64, output: &Path, bitrate: u32) -> Result<Mixed, String> {
    for sound in sounds {
        let path = folder.join(&sound.source);
        if !path.is_file() {
            return Err(format!(
                "the sound `{}` of <audio id=\"{}\"> was not found",
                path.display(),
                sound.id
            ));
        }
    }
    let frames = (seconds * f64::from(RATE)).round() as usize;
    let mut playing: Vec<Playing> = sounds
        .iter()
        .map(|sound| {
            let first = (sound.start * f64::from(RATE)).round() as usize;
            let wanted = (sound.duration * f64::from(RATE)).round() as usize;
            Playing {
                sound,
                first,
                wanted: wanted.min(frames.saturating_sub(first)),
                done: 0,
                after: 0,
                decoder: None,
                over: false,
            }
        })
        .collect();
    // The mix as made goes to a file, since its peak is known only at its end.
    let made = output.with_extension("mix.f32");
    let result = mixed(folder, &mut playing, frames, &made).and_then(|peak| {
        let gain = lowering(peak);
        encode(&made, gain.unwrap_or(1.0), output, bitrate)?;
        Ok(Mixed {
            peak,
            lowered: gain.map_or(0.0, |gain| -20.0 * gain.log10()),
        })
    });
    for sound in &mut playing {
        sound.end();
    }
    let _ = std::fs::remove_file(&made);
    result
}

/// Makes the mix stretch by stretch into the file `made` and returns its highest peak.
fn mixed(folder: &Path, playing: &mut [Playing], frames: usize, made: &Path) -> Result<f32, String> {
    let file = std::fs::File::create(made).map_err(|error| format!("{}: {error}", made.display()))?;
    let mut file = std::io::BufWriter::with_capacity(1 << 20, file);
    let mut peak = Peak::new();
    let mut stretch = Vec::with_capacity(STRETCH * 2);
    let mut out = Vec::with_capacity(STRETCH * 8);
    let mut from = 0;
    while from < frames {
        let length = STRETCH.min(frames - from);
        stretch.clear();
        stretch.resize(length * 2, 0.0_f32);
        // In the order the sounds are written, so that every sample is the same sum as ever.
        for sound in playing.iter_mut() {
            sound.add_to(&mut stretch, from, folder)?;
        }
        peak.add(&stretch);
        out.clear();
        for sample in &stretch {
            out.extend_from_slice(&sample.to_le_bytes());
        }
        file.write_all(&out).map_err(|error| format!("{}: {error}", made.display()))?;
        from += length;
    }
    file.flush().map_err(|error| format!("{}: {error}", made.display()))?;
    Ok(peak.highest())
}

/// Encodes the mix in the file `made`, each sample times `gain`, to AAC of `bitrate` kbit/s in
/// `output`.
fn encode(made: &Path, gain: f32, output: &Path, bitrate: u32) -> Result<(), String> {
    let failed = |error: frx7_base::Error| format!("the sound could not be encoded: {error}");
    let setup = AudioEncoderSetup {
        codec: Codec::Aac,
        sample_rate: RATE,
        channels: 2,
        bitrate: bitrate * 1000,
    };
    let mut file = std::fs::File::open(made).map_err(|error| format!("{}: {error}", made.display()))?;
    let frames = file.metadata().map(|facts| facts.len() / 8).ok();
    let (formats, codecs) = registries();
    let mut sound = SoundFile::create(output, &setup, frames, &formats, &codecs).map_err(failed)?;
    let mut bytes = vec![0_u8; 1 << 20];
    let mut samples = Vec::with_capacity(bytes.len() / 4);
    loop {
        let read = fill(&mut file, &mut bytes).map_err(|error| format!("{}: {error}", made.display()))?;
        if read == 0 {
            break;
        }
        samples.clear();
        samples.extend(
            bytes[..read]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|sample| (f32::from_le_bytes(*sample) * gain).clamp(-1.0, 1.0)),
        );
        sound.sound(&samples).map_err(failed)?;
    }
    sound.finish().map_err(failed)?;
    Ok(())
}

/// Reads until `bytes` is full or the file ends. Returns how many bytes were read.
fn fill(file: &mut std::fs::File, bytes: &mut [u8]) -> std::io::Result<usize> {
    let mut read = 0;
    while read < bytes.len() {
        match file.read(&mut bytes[read..]) {
            Ok(0) => break,
            Ok(count) => read += count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(read)
}
