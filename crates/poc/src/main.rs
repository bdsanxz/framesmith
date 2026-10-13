//! `poc render <project folder | framesmith.yaml | composition.xml> [--output <file.mp4>]
//! [--backend auto|cpu|gpu] [--quality draft|standard|high] [--encoder hardware|software] [--codec h264|hevc]
//! [--workers auto|<number>] [--no-encode] [--frames <first>:<end>] [--marks <file>] [--dump <folder>]`
//!
//! `poc seek <project folder | framesmith.yaml | composition.xml> --list <file>
//! [--backend auto|cpu|gpu] [--dump <folder>]`
//!
//! `poc check <project folder | framesmith.yaml | composition.xml>`

use poc::decode::{self, Asked, Decoder, Spent};
use poc::encode::{self, Encoder, Fed};
use poc::model::{Composition, Kind, Node};
use poc::project::{self, Codec, Coder, Project, Quality, Renderer, Workers};
use poc::scene::{Evaluator, Footage};
use poc::text::Shaper;
use poc::{audio, cpu::Cpu, layout, xml, yuv, Backend};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const USAGE: &str = "usage: poc render <project folder | framesmith.yaml | composition.xml> [--output <file.mp4>] \
[--backend auto|cpu|gpu] [--quality draft|standard|high] [--encoder hardware|software] [--codec h264|hevc] \
[--workers auto|<number>] [--no-encode] [--frames <first>:<end>] [--marks <file>] [--dump <folder>]
       poc seek <project folder | framesmith.yaml | composition.xml> --list <file> [--backend auto|cpu|gpu] [--dump <folder>]
       poc check <project folder | framesmith.yaml | composition.xml>";

struct Args {
    /// Whether frames are drawn on request (`seek`) and not in order (`render`).
    seek: bool,
    /// For `seek`: the file that lists the frames asked for.
    list: Option<PathBuf>,
    /// For `render`: the file the mark of each frame's pixels is written to.
    marks: Option<PathBuf>,
    /// The folder each frame's RGBA pixels are written to, a file for each frame.
    dump: Option<PathBuf>,
    /// A project folder, a project file or a composition.
    target: PathBuf,
    output: Option<PathBuf>,
    backend: Option<Renderer>,
    quality: Option<Quality>,
    coder: Option<Coder>,
    codec: Option<Codec>,
    workers: Option<Workers>,
    encode: bool,
    /// The first frame drawn and the frame after the last; the whole length when absent.
    frames: Option<(u64, u64)>,
}

fn args() -> Option<Args> {
    let mut words = std::env::args().skip(1);
    let seek = match words.next()?.as_str() {
        "render" => false,
        "seek" => true,
        _ => return None,
    };
    let mut args = Args {
        seek,
        list: None,
        marks: None,
        dump: None,
        target: PathBuf::new(),
        output: None,
        backend: None,
        quality: None,
        coder: None,
        codec: None,
        workers: None,
        encode: true,
        frames: None,
    };
    let mut target = None;
    while let Some(word) = words.next() {
        match word.as_str() {
            "--output" => args.output = Some(PathBuf::from(words.next()?)),
            "--backend" => args.backend = Some(Renderer::from_name(&words.next()?)?),
            "--quality" => args.quality = Some(Quality::from_name(&words.next()?)?),
            "--encoder" => args.coder = Some(Coder::from_name(&words.next()?)?),
            "--codec" => args.codec = Some(Codec::from_name(&words.next()?)?),
            "--workers" => args.workers = Some(Workers::from_name(&words.next()?)?),
            "--no-encode" => args.encode = false,
            "--list" => args.list = Some(PathBuf::from(words.next()?)),
            "--marks" => args.marks = Some(PathBuf::from(words.next()?)),
            "--dump" => args.dump = Some(PathBuf::from(words.next()?)),
            "--frames" => {
                let range = words.next()?;
                let (first, end) = range.split_once(':')?;
                args.frames = Some((first.parse().ok()?, end.parse().ok()?));
            }
            _ if target.is_none() && !word.starts_with("--") => target = Some(PathBuf::from(word)),
            _ => return None,
        }
    }
    args.target = target?;
    // `seek` needs its list; marks are of RGBA pixels, which a render that encodes may not make.
    let rgba_of_a_render = args.marks.is_some() || (args.dump.is_some() && !args.seek);
    if args.seek != args.list.is_some() || (args.marks.is_some() && args.seek) || (rgba_of_a_render && args.encode) {
        return None;
    }
    Some(args)
}

fn ms(time: Duration) -> f64 {
    time.as_secs_f64() * 1000.0
}

/// A mark of a frame's pixels: equal pixels give equal marks.
fn mark(pixels: &[u8]) -> u64 {
    let (words, rest) = pixels.as_chunks::<8>();
    let mut mark = 0x9e37_79b9_7f4a_7c15_u64;
    for word in words {
        mark = (mark ^ u64::from_le_bytes(*word))
            .wrapping_mul(0x0000_0100_0000_01b3)
            .rotate_left(23);
    }
    for byte in rest {
        mark = (mark ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3).rotate_left(23);
    }
    mark
}

/// The largest size each still is drawn at by layout alone, by its place in the stills.
fn drawn_sizes(node: &Node, sizes: &mut [(f64, f64)]) {
    if let Kind::Image { still, .. } = &node.kind {
        if let Some(size) = sizes.get_mut(*still) {
            *size = (size.0.max(node.rect.width()), size.1.max(node.rect.height()));
        }
    }
    for child in &node.children {
        drawn_sizes(child, sizes);
    }
}

/// One video of the composition: its decoder while it is in use.
/// What the sound thread reports: the highest peak of the mix as written, by how many decibels
/// the mix was lowered, and its time.
type Sounded = (f32, f32, Duration);

#[derive(Default)]
struct Stream {
    decoder: Option<Decoder>,
    /// The frame it was last read for.
    used: u64,
    /// Whether its decoder hands over YUV planes for the renderer to turn into RGBA.
    raw: bool,
    /// The first picture its decoder was started for. The pictures a decoder gives before it
    /// are only there to be passed over.
    sure_from: u64,
}

/// Starts the decoder of a video at picture `first` of its source.
fn begin(stream: &mut Stream, footage: &Footage, first: u64, job: &Job, backend: &dyn Backend) -> Result<(), String> {
    let (folder, fps, spent) = (job.folder, job.composition.fps, job.spent);
    let Kind::Video { source, filter, .. } = &footage.node.kind else {
        return Ok(());
    };
    let path = folder.join(source);
    if !path.is_file() {
        return Err(format!(
            "the video `{}` of <video id=\"{}\"> was not found",
            path.display(),
            footage.node.id
        ));
    }
    let size = (footage.node.rect.width() as u32, footage.node.rect.height() as u32);
    if !job.sources.contains_key(&path) {
        return Err(format!("the video `{}` was not looked at before the render", path.display()));
    }
    stream.raw = backend.takes_planes(filter) && yuv::fits(size.0 as usize, size.1 as usize);
    let asked = Asked {
        size,
        fps,
        first,
        filter,
        raw: stream.raw,
    };
    let started = Decoder::start(&path, &asked, spent);
    stream.decoder = Some(started.map_err(|e| format!("the video `{}` could not be decoded: {e}", path.display()))?);
    Ok(())
}

/// Puts the pictures of footage a frame shows where the renderer draws them from. Returns how
/// many pictures were taken and how many decoders were started.
fn take_footage(
    footage: &[Footage],
    streams: &mut [Stream],
    backend: &mut dyn Backend,
    job: &Job,
    frame: u64,
) -> Result<(u64, u64), String> {
    let (mut decoded, mut restarts) = (0, 0);
    for footage in footage {
        let (width, height) = (footage.node.rect.width() as u32, footage.node.rect.height() as u32);
        let stream = &mut streams[footage.video];
        let wanted = footage.source_frame;
        // A decoder reads forward only. One that is far behind, because its video was
        // hidden for a while, is started again where it is wanted.
        let usable = stream
            .decoder
            .as_ref()
            .is_some_and(|d| wanted + 1 >= d.next() && wanted <= d.next() + 30);
        if !usable {
            begin(stream, footage, wanted, job, &*backend)?;
            restarts += 1;
        }
        stream.used = stream.used.max(frame);
        let buffer = match &footage.node.kind {
            Kind::Video { filter, .. } if stream.raw => backend.video_planes(footage.video, width, height, filter),
            _ => backend.video_buffer(footage.video, width, height),
        };
        let Some(decoder) = stream.decoder.as_mut() else { continue };
        let changed = decoder
            .read(wanted, buffer)
            .map_err(|e| format!("frame {frame}: the footage could not be read: {e}"))?;
        decoded += u64::from(changed);
    }
    Ok((decoded, restarts))
}

/// The file of every video of the composition, with the line and id of the first element
/// that shows it.
fn videos(node: &Node, folder: &Path, found: &mut Vec<(PathBuf, u32, String)>) {
    if let Kind::Video { source, .. } = &node.kind {
        let path = folder.join(source);
        if !found.iter().any(|(known, ..)| *known == path) {
            found.push((path, node.line, node.id.clone()));
        }
    }
    for child in &node.children {
        videos(child, folder, found);
    }
}

/// Looks at every video source of the composition, several at a time. A source that is missing
/// or of a kind that would be shown wrongly stops the render here, before a frame is drawn.
fn sources(composition: &Composition, folder: &Path) -> Result<Sources, String> {
    let mut found = Vec::new();
    videos(&composition.root, folder, &mut found);
    for (path, _, id) in &found {
        if !path.is_file() {
            return Err(format!("the video `{}` of <video id=\"{id}\"> was not found", path.display()));
        }
    }
    let mut looked: Vec<Option<Result<decode::Source, String>>> = found.iter().map(|_| None).collect();
    let share = found.len().div_ceil(8).max(1);
    std::thread::scope(|scope| {
        for (slots, group) in looked.chunks_mut(share).zip(found.chunks(share)) {
            scope.spawn(move || {
                for (slot, (path, ..)) in slots.iter_mut().zip(group) {
                    *slot = Some(decode::look(path));
                }
            });
        }
    });
    let mut sources = Sources::new();
    for ((path, line, id), looked) in found.into_iter().zip(looked) {
        let source = looked
            .unwrap_or_else(|| Err("the source was not looked at".to_string()))
            .map_err(|reason| format!("line {line}: <video id=\"{id}\">: {reason}"))?;
        sources.insert(path, source);
    }
    Ok(sources)
}

/// What is known of each video source, by its file.
type Sources = std::collections::HashMap<PathBuf, decode::Source>;

fn load(file: &Path, folder: &Path) -> Result<(Composition, Vec<decode::Picture>, Sources, [Duration; 4]), String> {
    let began = Instant::now();
    let mut composition = xml::parse_file(file, folder).map_err(|problems| {
        let lines: Vec<String> = problems.iter().map(|problem| format!("  {problem}")).collect();
        format!("{} is not a valid composition:\n{}", file.display(), lines.join("\n"))
    })?;
    let sources = sources(&composition, folder)?;
    let parsed = began.elapsed();

    let mut shaper = Shaper::new();
    for font in &composition.fonts {
        let path = folder.join(&font.source);
        let bytes = std::fs::read(&path).map_err(|e| format!("line {}: the font `{}`: {e}", font.line, path.display()))?;
        shaper
            .register(bytes, font.family.as_deref())
            .map_err(|e| format!("line {}: the font `{}`: {e}", font.line, path.display()))?;
    }
    let fonts = began.elapsed() - parsed;

    // Layout needs the size of each picture; the pixels are then read at the size they are drawn.
    let natural = decode::sizes(folder, &composition.stills)?;
    layout::lay_out(&mut composition, &mut shaper, &natural);
    let laid_out = began.elapsed() - parsed - fonts;
    let mut drawn = vec![(0.0, 0.0); composition.stills.len()];
    drawn_sizes(&composition.root, &mut drawn);
    let targets: Vec<(u32, u32)> = natural
        .iter()
        .zip(&drawn)
        .map(|(natural, drawn)| {
            let (width, height) = (drawn.0.ceil() as u32, drawn.1.ceil() as u32);
            let smaller = width > 0 && height > 0 && f64::from(width) < f64::from(natural.0) * 0.9;
            if smaller {
                (width, height)
            } else {
                *natural
            }
        })
        .collect();
    let pictures = decode::stills(folder, &composition.stills, &targets)?;
    let stills = began.elapsed() - parsed - fonts - laid_out;
    Ok((composition, pictures, sources, [parsed, fonts, laid_out, stills]))
}

/// What every worker of a render shares.
struct Job<'a> {
    composition: &'a Composition,
    /// The still pictures, decoded.
    pictures: &'a [decode::Picture],
    folder: &'a Path,
    renderer: Renderer,
    quality: Quality,
    coder: Coder,
    codec: Codec,
    /// What the decoders' threads spend beside the frame loops.
    spent: &'a Arc<Spent>,
    /// Set when a worker fails, so that the others do not go on for nothing.
    stop: &'a AtomicBool,
    /// What is known of each video source.
    sources: &'a Sources,
    /// Whether the mark of each frame's pixels is kept.
    marks: bool,
    /// The folder each frame's pixels are written to.
    dump: Option<&'a Path>,
}

/// Writes the pixels of a frame to `<name>.rgba` in the folder.
fn dump(folder: &Path, name: &str, pixels: &[u8]) -> Result<(), String> {
    let file = folder.join(format!("{name}.rgba"));
    std::fs::create_dir_all(folder)
        .and_then(|()| std::fs::write(&file, pixels))
        .map_err(|e| format!("{}: {e}", file.display()))
}

/// What one worker did, or all of them together.
#[derive(Default)]
struct Tally {
    backend: &'static str,
    backend_started: Duration,
    evaluate: Duration,
    /// Waiting for footage and taking it.
    footage: Duration,
    draw: Duration,
    pixels: Duration,
    feed: Duration,
    /// Waiting for the encoder to finish its file.
    finished: Duration,
    decoded: u64,
    restarts: u64,
    steps: u64,
    fed: Option<Fed>,
    /// Each frame's number and the mark of its pixels, when marks are kept.
    marks: Option<Vec<(u64, u64)>>,
}

impl Tally {
    fn add(&mut self, other: &Tally) {
        self.backend = other.backend;
        self.backend_started += other.backend_started;
        self.evaluate += other.evaluate;
        self.footage += other.footage;
        self.draw += other.draw;
        self.pixels += other.pixels;
        self.feed += other.feed;
        self.finished += other.finished;
        self.decoded += other.decoded;
        self.restarts += other.restarts;
        self.steps += other.steps;
        if let Some(theirs) = &other.marks {
            self.marks.get_or_insert_with(Vec::new).extend(theirs);
        }
        if let Some(theirs) = other.fed {
            let ours = self.fed.get_or_insert_with(Fed::default);
            ours.converting += theirs.converting;
            ours.writing += theirs.writing;
        }
    }
}

fn backend(job: &Job) -> Result<Box<dyn Backend>, String> {
    let (composition, pictures) = (job.composition, job.pictures);
    // The CPU renderer, which cannot draw an opening written as a shader: a composition that
    // has one is refused here, before any frame.
    let cpu = || -> Result<Box<dyn Backend>, String> {
        match composition.shaders.first() {
            Some(shader) => Err(format!(
                "the transition `{}` is a shader, which only the GPU renderer draws; render with `--backend gpu`",
                shader.name
            )),
            None => Ok(Box::new(Cpu::new(composition, pictures))),
        }
    };
    Ok(match job.renderer {
        Renderer::Cpu => cpu()?,
        #[cfg(feature = "gpu")]
        Renderer::Gpu => Box::new(poc::gpu::Gpu::new(composition, pictures)?),
        // The GPU renderer when the build has it and an adapter answers; the CPU renderer if not.
        // A shader the card does not take is the composition's fault, not the card's.
        #[cfg(feature = "gpu")]
        Renderer::Auto => match poc::gpu::Gpu::new(composition, pictures) {
            Ok(gpu) => Box::new(gpu),
            Err(reason) => match composition.shaders.first() {
                Some(shader) => {
                    return Err(format!(
                        "{reason}\nthe transition `{}` is a shader and needs the GPU renderer",
                        shader.name
                    ))
                }
                None => cpu()?,
            },
        },
        #[cfg(not(feature = "gpu"))]
        Renderer::Auto => cpu()?,
        #[cfg(not(feature = "gpu"))]
        Renderer::Gpu => return Err("this build has no GPU renderer; use `--backend cpu`".to_string()),
    })
}

/// One worker: draws frames `first..end` with a renderer, decoders and an encoder of its own
/// and, with `output`, encodes them into that file.
fn part(job: &Job, first: u64, end: u64, output: Option<&Path>) -> Result<Tally, String> {
    let composition = job.composition;
    let fps = composition.fps;
    let evaluator = Evaluator::new(composition);
    let mut tally = Tally {
        marks: job.marks.then(Vec::new),
        ..Tally::default()
    };
    let before_backend = Instant::now();
    let mut backend = backend(job)?;
    tally.backend = backend.name();
    tally.backend_started = before_backend.elapsed();
    let mut encoder = match output {
        Some(output) => {
            let started = Encoder::start(composition, output, job.quality, job.coder, job.codec, end - first);
            Some(started.map_err(|e| format!("the encoder could not be started: {e}"))?)
        }
        None => None,
    };
    // A renderer that can make the encoder's planes itself is asked to.
    if encoder.as_ref().is_some_and(Encoder::takes_planes) {
        backend.hand_over_planes();
    }
    let mut streams: Vec<Stream> = (0..composition.videos).map(|_| Stream::default()).collect();
    // The frames drawn whose pixels have not been taken yet. A renderer that lets frames be on
    // their way together draws the next one while the one before is finished.
    let ahead = backend.ahead() as u64;
    let mut on_their_way = 0_u64;
    for frame in first..end {
        if job.stop.load(Ordering::Relaxed) {
            return Err("stopped, because another part of the render failed".to_string());
        }
        let t0 = Instant::now();
        let described = evaluator.frame(frame);
        let before_decode = Instant::now();
        let (decoded, restarts) = take_footage(&described.footage, &mut streams, &mut *backend, job, frame)?;
        tally.decoded += decoded;
        tally.restarts += restarts;
        // A video that comes on screen within a second has its decoder started now, so that
        // its first picture is ready when its frame is drawn. At the first frame the whole
        // second is looked through; after that, only the frame that has just come in reach.
        // This comes after the frame's own footage: a video on screen now starts at the
        // picture this frame shows.
        let before_glance = Instant::now();
        let reach = u64::from(fps);
        let glance = if frame == first {
            frame + 1..=frame + reach
        } else {
            frame + reach..=frame + reach
        };
        for coming in glance.filter(|coming| *coming < end) {
            for footage in &evaluator.frame(coming).footage {
                let stream = &mut streams[footage.video];
                if stream.decoder.is_none() {
                    begin(stream, footage, footage.source_frame, job, &*backend)?;
                    stream.used = coming;
                    tally.restarts += 1;
                }
            }
        }
        let glanced = before_glance.elapsed();
        // A video not shown for two seconds has had its turn.
        for (video, stream) in streams.iter_mut().enumerate() {
            if stream.decoder.is_some() && stream.used + u64::from(fps) * 2 < frame {
                stream.decoder = None;
                backend.release_video(video);
            }
        }
        let t1 = Instant::now();
        tally.footage += t1 - before_decode - glanced;
        tally.steps += described.ops.len() as u64;
        backend.draw(frame, &described.ops).map_err(|e| format!("frame {frame}: {e}"))?;
        on_their_way += 1;
        tally.evaluate += before_decode - t0 + glanced;
        tally.draw += t1.elapsed();
        if on_their_way == ahead {
            finish(&mut *backend, encoder.as_mut(), frame + 1 - on_their_way, &mut tally, job.dump)?;
            on_their_way -= 1;
        }
    }
    while on_their_way > 0 {
        finish(&mut *backend, encoder.as_mut(), end - on_their_way, &mut tally, job.dump)?;
        on_their_way -= 1;
    }
    drop(streams);
    let before_finish = Instant::now();
    if let Some(encoder) = encoder {
        tally.fed = Some(encoder.finish().map_err(|e| format!("the video could not be finished: {e}"))?);
    }
    tally.finished = before_finish.elapsed();
    Ok(tally)
}

/// Takes the pixels of the oldest frame on its way, which is frame `frame`, and hands them to
/// the encoder.
fn finish(
    backend: &mut dyn Backend,
    encoder: Option<&mut Encoder>,
    frame: u64,
    tally: &mut Tally,
    folder: Option<&Path>,
) -> Result<(), String> {
    let began = Instant::now();
    let picture = backend.pixels().map_err(|e| format!("frame {frame}: {e}"))?;
    let taken = Instant::now();
    if let Some(marks) = tally.marks.as_mut() {
        marks.push((frame, mark(picture)));
    }
    if let Some(folder) = folder {
        dump(folder, &frame.to_string(), picture)?;
    }
    if let Some(encoder) = encoder {
        encoder
            .frame(picture)
            .map_err(|e| format!("frame {frame} could not be given to the encoder: {e}"))?;
    }
    tally.pixels += taken - began;
    tally.feed += taken.elapsed();
    Ok(())
}

/// Frames `first..end` shared out over at most `workers` parts, each of at least a second.
fn parts(first: u64, end: u64, workers: u32, fps: u32) -> Vec<(u64, u64)> {
    let frames = end - first;
    let count = u64::from(workers).min(frames / u64::from(fps.max(1))).max(1);
    (0..count)
        .map(|part| (first + frames * part / count, first + frames * (part + 1) / count))
        .collect()
}

fn run(args: &Args) -> Result<(), String> {
    let began = Instant::now();
    let (project, file): (Project, PathBuf) = project::open(&args.target)?;
    let folder = project.folder.clone();
    let output = args.output.clone().unwrap_or_else(|| project.output_path());
    let (composition, pictures, sources, [parsed, fonts, laid_out, stills]) = load(&file, &folder)?;
    let all = timebase::frame_count(composition.duration, composition.fps);
    let (first, end) = args.frames.map_or((0, all), |(first, end)| (first.min(all), end.min(all)));
    let whole = first == 0 && end == all;
    let renderer = args.backend.unwrap_or(project.renderer);
    let workers = args.workers.unwrap_or(project.workers).count(renderer);
    let ranges = parts(first, end, workers, composition.fps);

    // The sound is decoded, mixed and encoded on its own thread while the frames are drawn.
    let with_sound = whole && args.encode && !composition.audio.is_empty();
    let picture_file = if with_sound {
        output.with_extension("picture.mp4")
    } else {
        output.clone()
    };
    let sound_file = output.with_extension("sound.m4a");
    if args.encode {
        if let Some(parent) = output.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
    }
    // One worker writes the picture file itself; several write a part each.
    let files: Vec<PathBuf> = match ranges.len() {
        1 => vec![picture_file.clone()],
        count => (0..count).map(|part| output.with_extension(format!("part-{part}.mp4"))).collect(),
    };
    let audio_bitrate = project.audio_bitrate;
    let spent = Arc::new(Spent::default());
    let stop = AtomicBool::new(false);
    let job = Job {
        composition: &composition,
        pictures: &pictures,
        folder: &folder,
        renderer,
        quality: args.quality.unwrap_or(project.quality),
        coder: args.coder.unwrap_or(project.coder),
        codec: args.codec.unwrap_or(project.codec),
        spent: &spent,
        stop: &stop,
        sources: &sources,
        marks: args.marks.is_some(),
        dump: args.dump.as_deref(),
    };
    let work = |range: &(u64, u64), file: &PathBuf| -> Result<Tally, String> {
        let done = part(&job, range.0, range.1, args.encode.then_some(file.as_path()));
        if done.is_err() {
            job.stop.store(true, Ordering::Relaxed);
        }
        done
    };
    let loop_began = Instant::now();
    let (done, looped, sound) = std::thread::scope(|scope| {
        let sound = with_sound.then(|| {
            let (composition, folder, sound_file) = (&composition, &folder, &sound_file);
            scope.spawn(move || -> Result<Sounded, String> {
                let began = Instant::now();
                // As long as the picture: whole frames.
                let seconds = all as f64 / f64::from(composition.fps);
                let mix = audio::render(folder, &composition.audio, seconds, sound_file, audio_bitrate)?;
                Ok((mix.peak, mix.lowered, began.elapsed()))
            })
        });
        let work = &work;
        let others: Vec<_> = ranges
            .iter()
            .zip(&files)
            .skip(1)
            .map(|(range, file)| scope.spawn(move || work(range, file)))
            .collect();
        let mut done = vec![work(&ranges[0], &files[0])];
        for other in others {
            done.push(other.join().unwrap_or_else(|_| Err("a part of the render stopped".to_string())));
        }
        let looped = loop_began.elapsed();
        let sound = sound.map(|thread| thread.join().unwrap_or_else(|_| Err("the sound thread stopped".to_string())));
        (done, looped, sound)
    });
    // The first failure that is not just a worker told to stop.
    let mut tally = Tally::default();
    let mut failed: Option<String> = None;
    for part in done {
        match part {
            Ok(part) => tally.add(&part),
            Err(message) if failed.is_none() || !message.starts_with("stopped") => failed = Some(message),
            Err(_) => {}
        }
    }
    if let Some(message) = failed {
        return Err(message);
    }
    let sound = sound.transpose()?;
    drop(pictures);
    if let (Some(file), Some(marks)) = (&args.marks, tally.marks.as_mut()) {
        marks.sort_unstable();
        let lines: String = marks.iter().map(|(frame, mark)| format!("{frame} {mark:016x}\n")).collect();
        std::fs::write(file, lines).map_err(|e| format!("{}: {e}", file.display()))?;
    }

    let before_join = Instant::now();
    if args.encode && (files.len() > 1 || with_sound) {
        // The parts of the picture and the sound go into the finished file in one pass.
        let joined = encode::join(&files, with_sound.then_some(sound_file.as_path()), &output);
        for file in &files {
            let _ = std::fs::remove_file(file);
        }
        let _ = std::fs::remove_file(&sound_file);
        joined.map_err(|e| format!("the parts of the film could not be joined: {e}"))?;
    }
    let joined = before_join.elapsed();

    let frames = (end - first).max(1);
    let each = |time: Duration| ms(time) / frames as f64;
    if args.encode {
        println!("Wrote {}", output.display());
    }
    println!(
        "Rendered {} frames, {}x{}, {} fps, backend {}, {} at once, in {:.2} s",
        end - first,
        composition.width,
        composition.height,
        composition.fps,
        tally.backend,
        match ranges.len() {
            1 => "1 worker".to_string(),
            count => format!("{count} workers"),
        },
        began.elapsed().as_secs_f64()
    );
    println!("  parse and validate      {:8.2} ms", ms(parsed));
    println!("  fonts                   {:8.2} ms", ms(fonts));
    println!("  layout                  {:8.2} ms", ms(laid_out));
    println!(
        "  still pictures          {:8.2} ms  ({} pictures)",
        ms(stills),
        composition.stills.len()
    );
    println!("  frames                  {:8.2} ms  ({:.3} ms a frame)", ms(looped), each(looped));
    println!("  in the workers' frame loops, added up over the workers:");
    println!("    start the renderer    {:8.2} ms", ms(tally.backend_started));
    println!("    evaluate the timeline {:8.3} ms a frame", each(tally.evaluate));
    println!(
        "    take the footage      {:8.3} ms a frame  ({} pictures taken, {} decoder starts)",
        each(tally.footage),
        tally.decoded,
        tally.restarts
    );
    println!(
        "    describe the frame    {:8.3} ms a frame  ({:.1} steps a frame)",
        each(tally.draw),
        tally.steps as f64 / frames as f64
    );
    println!("    produce the pixels    {:8.3} ms a frame", each(tally.pixels));
    println!("    hand to the encoder   {:8.3} ms a frame", each(tally.feed));
    println!("    wait for the encoder  {:8.2} ms", ms(tally.finished));
    println!("  beside the frame loops, on threads of their own:");
    let beside = |spent: &std::sync::atomic::AtomicU64| each(Duration::from_nanos(spent.load(Ordering::Relaxed)));
    println!("    read the footage      {:8.3} ms a frame", beside(&spent.reading));
    println!("    make it RGBA, coloured {:7.3} ms a frame", beside(&spent.colouring));
    if let Some(fed) = tally.fed {
        println!("    convert for the encoder {:6.3} ms a frame", each(fed.converting));
        println!("    write to the encoder  {:8.3} ms a frame", each(fed.writing));
    }
    if let Some((peak, lowered, time)) = sound {
        let level = if lowered > 0.0 {
            format!("lowered by {lowered:.2} dB to stay under -1 dBTP")
        } else {
            "not lowered".to_string()
        };
        println!(
            "  sound, on its own thread {:7.2} ms  ({} sounds, true peak of the mix {:+.2} dBTP, {level})",
            ms(time),
            composition.audio.len(),
            20.0 * peak.max(1e-9).log10()
        );
    }
    if args.encode && (files.len() > 1 || with_sound) {
        println!("  join the parts and the sound {:6.2} ms", ms(joined));
    }
    Ok(())
}

/// For how many requests after the last that showed it a video keeps its decoder.
const KEPT_FOR: u64 = 8;
/// How many pictures of a video are kept for a step back.
const KEPT: usize = 64;
/// How many pictures before the one wanted a decoder starts at after a step back, so that the
/// next steps back find their pictures kept.
const BACK: u64 = 45;
/// How far ahead of its decoder a picture may be for the decoder to read on to it.
const REACH: u64 = 90;

/// The pictures of a video kept while it is asked for on request, each with its place in the
/// source, oldest first.
type Kept = std::collections::VecDeque<(u64, Vec<u8>)>;

/// Puts the pictures of footage a frame asked for on request shows where the renderer draws
/// them from. Returns how many decoders were started.
fn footage_on_request(
    footage: &[Footage],
    streams: &mut [Stream],
    kept: &mut [Kept],
    backend: &mut dyn Backend,
    job: &Job,
    (frame, request): (u64, u64),
) -> Result<u64, String> {
    let mut starts = 0;
    for footage in footage {
        let (width, height) = (footage.node.rect.width() as u32, footage.node.rect.height() as u32);
        let (stream, kept) = (&mut streams[footage.video], &mut kept[footage.video]);
        let wanted = footage.source_frame;
        stream.used = request;
        let found = kept.iter().any(|(place, _)| *place == wanted);
        if !found {
            let next = stream.decoder.as_ref().map(Decoder::next);
            if !next.is_some_and(|next| wanted >= next && wanted <= next + REACH) {
                // After a step back the decoder starts early; after a jump, at the picture.
                let stepped_back = next.is_some_and(|next| wanted < next && next - wanted <= KEPT as u64);
                let first = if stepped_back { wanted.saturating_sub(BACK) } else { wanted };
                begin(stream, footage, first, job, &*backend)?;
                stream.sure_from = first;
                starts += 1;
            }
        }
        let buffer = match &footage.node.kind {
            Kind::Video { filter, .. } if stream.raw => backend.video_planes(footage.video, width, height, filter),
            _ => backend.video_buffer(footage.video, width, height),
        };
        if let Some((_, picture)) = kept.iter().find(|(place, _)| *place == wanted) {
            buffer.copy_from_slice(picture);
            continue;
        }
        let Some(decoder) = stream.decoder.as_mut() else { continue };
        // Every picture up to the one wanted is read and kept; the last one stays in the buffer.
        while decoder.next() <= wanted {
            let place = decoder.next();
            let changed = decoder
                .read(place, buffer)
                .map_err(|e| format!("frame {frame}: the footage could not be read: {e}"))?;
            if !changed {
                break;
            }
            if place < stream.sure_from {
                continue;
            }
            let mut picture = if kept.len() == KEPT {
                kept.pop_front().map_or_else(Vec::new, |(_, picture)| picture)
            } else {
                Vec::new()
            };
            picture.clear();
            picture.extend_from_slice(buffer);
            kept.push_back((place, picture));
        }
    }
    Ok(starts)
}

/// Holds the composition open and draws the frames a list names, in the order of the list.
fn seek(args: &Args) -> Result<(), String> {
    let began = Instant::now();
    let (project, file): (Project, PathBuf) = project::open(&args.target)?;
    let folder = project.folder.clone();
    let (composition, pictures, sources, _) = load(&file, &folder)?;
    let all = timebase::frame_count(composition.duration, composition.fps);
    let list = args.list.as_deref().unwrap_or(Path::new(""));
    let text = std::fs::read_to_string(list).map_err(|e| format!("{}: {e}", list.display()))?;
    let mut asked = Vec::new();
    for (line, word) in text.lines().enumerate().filter(|(_, word)| !word.trim().is_empty()) {
        match word.trim().parse::<u64>() {
            Ok(frame) if frame < all => asked.push(frame),
            _ => {
                return Err(format!(
                    "{}, line {}: `{}` is not a frame of this composition, which has frames 0 to {}",
                    list.display(),
                    line + 1,
                    word.trim(),
                    all.saturating_sub(1)
                ))
            }
        }
    }
    let spent = Arc::new(Spent::default());
    let stop = AtomicBool::new(false);
    let job = Job {
        composition: &composition,
        pictures: &pictures,
        folder: &folder,
        renderer: args.backend.unwrap_or(project.renderer),
        quality: project.quality,
        coder: project.coder,
        codec: project.codec,
        spent: &spent,
        stop: &stop,
        sources: &sources,
        marks: false,
        dump: None,
    };
    let evaluator = Evaluator::new(&composition);
    let mut backend = backend(&job)?;
    let mut streams: Vec<Stream> = (0..composition.videos).map(|_| Stream::default()).collect();
    let mut kept: Vec<Kept> = (0..composition.videos).map(|_| Kept::new()).collect();
    println!(
        "Opened {}: {} frames, {}x{}, {} fps, backend {}, in {:.2} ms",
        file.display(),
        all,
        composition.width,
        composition.height,
        composition.fps,
        backend.name(),
        ms(began.elapsed())
    );
    let mut times = Vec::with_capacity(asked.len());
    for (request, frame) in (1_u64..).zip(asked) {
        let began = Instant::now();
        let described = evaluator.frame(frame);
        let starts = footage_on_request(&described.footage, &mut streams, &mut kept, &mut *backend, &job, (frame, request))?;
        for (video, stream) in streams.iter_mut().enumerate() {
            if stream.decoder.is_some() && stream.used + KEPT_FOR < request {
                stream.decoder = None;
                kept[video].clear();
                backend.release_video(video);
            }
        }
        backend.draw(frame, &described.ops).map_err(|e| format!("frame {frame}: {e}"))?;
        let pixels = backend.pixels().map_err(|e| format!("frame {frame}: {e}"))?;
        let time = began.elapsed();
        if let Some(folder) = &args.dump {
            dump(folder, &format!("{request}-{frame}"), pixels)?;
        }
        println!(
            "frame {frame:>7}  {:9.2} ms  {starts} decoder starts  {:016x}",
            ms(time),
            mark(pixels)
        );
        times.push(time);
    }
    if !times.is_empty() {
        times.sort_unstable();
        let at = |share: f64| ms(times[((times.len() - 1) as f64 * share).round() as usize]);
        println!(
            "{} frames: median {:.2} ms, nine in ten under {:.2} ms, slowest {:.2} ms",
            times.len(),
            at(0.5),
            at(0.9),
            at(1.0)
        );
    }
    Ok(())
}

/// `poc check`: reads the composition as a render does, draws nothing, and says what a render
/// of it needs. See `docs/spec/native-render-pipeline.md`, section 8.
fn check(target: &Path) -> Result<(), String> {
    let (project, file) = project::open(target)?;
    let (composition, pictures, ..) = load(&file, &project.folder)?;
    println!(
        "{} is a valid composition: {} frames, {}x{}, {} fps",
        file.display(),
        timebase::frame_count(composition.duration, composition.fps),
        composition.width,
        composition.height,
        composition.fps
    );
    for shader in &composition.shaders {
        println!(
            "line {}: the transition `{}` is a shader: this composition can be rendered only with the GPU renderer, and its picture is not checked against the CPU renderer",
            shader.line, shader.name
        );
    }
    if composition.shaders.is_empty() {
        return Ok(());
    }
    // The card's compiler has its say now, not at the first frame of a render.
    #[cfg(feature = "gpu")]
    return poc::gpu::Gpu::new(&composition, &pictures)
        .map(|_| ())
        .map_err(|reason| format!("this composition cannot be rendered on this machine: {reason}"));
    #[cfg(not(feature = "gpu"))]
    {
        let _ = pictures;
        Err("this composition cannot be rendered by this build, which has no GPU renderer".to_string())
    }
}

fn main() -> ExitCode {
    let words: Vec<String> = std::env::args().skip(1).collect();
    let done = match &words[..] {
        [command, target] if command == "check" && !target.starts_with("--") => check(Path::new(target)),
        _ => {
            let Some(args) = args() else {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            };
            if args.seek {
                seek(&args)
            } else {
                run(&args)
            }
        }
    };
    match done {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
