//! A render with footage and sound where no other program can be found: the sources are made
//! by frx7, `poc` runs with a search path that holds nothing, and the finished file is read
//! back by frx7. Nothing in this test starts `ffmpeg`.

use frx7_base::{Codec, Color, Picture, Rational};
use frx7_codec::{Hardware, RateControl, VideoEncoderSetup};
use frx7_export::{Film, FilmSetup, Footage, Form, Pace};
use poc::encode::registries;
use std::path::{Path, PathBuf};
use std::process::Command;

const WIDTH: u32 = 64;
const HEIGHT: u32 = 32;
const FRAMES: u32 = 30;
/// Samples a second of the tone.
const RATE: u32 = 48_000;
/// How high the tone reaches, of full scale.
const TONE: f32 = 0.5;

/// A new, empty folder for one test, under the build's own temporary folder.
fn folder(name: &str) -> PathBuf {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("render-alone").join(name);
    if folder.exists() {
        std::fs::remove_dir_all(&folder).expect("the old test folder is removed");
    }
    std::fs::create_dir_all(&folder).expect("the test folder is made");
    folder
}

/// The luma of picture `index` of the clip: a flat grey, brighter with every picture.
fn luma(index: u32) -> u8 {
    (40 + 4 * index) as u8
}

/// Writes a clip of one second at 30 pictures a second.
fn clip(path: &Path) {
    let (formats, codecs) = registries();
    let setup = FilmSetup {
        video: VideoEncoderSetup {
            codec: Codec::H264,
            width: WIDTH,
            height: HEIGHT,
            rate: Rational::new(30, 1),
            bits: 8,
            rate_control: RateControl::Bitrate(2_000_000),
            key_distance: 30,
            reorder: false,
            color: Color::BT709,
            hdr: frx7_base::Hdr::default(),
            hardware: Hardware::Required,
        },
        audio: None,
        frames: Some(u64::from(FRAMES)),
    };
    let mut film = Film::create(path, &setup, &formats, &codecs).expect("the clip is started");
    for index in 0..FRAMES {
        let mut picture = Picture::new(film.format(), WIDTH, HEIGHT);
        picture.planes[0].data.fill(luma(index));
        picture.planes[1].data.fill(128);
        film.picture(&picture).expect("the picture is taken");
    }
    film.finish().expect("the clip is finished");
}

/// Writes a second of a 440 Hz tone on two channels as 16-bit samples in a WAV file.
fn tone(path: &Path) {
    let samples: Vec<u8> = (0..RATE)
        .flat_map(|index| {
            let turn = 2.0 * std::f32::consts::PI * 440.0 * index as f32 / RATE as f32;
            let sample = ((TONE * turn.sin()) * 32_767.0).round() as i16;
            [sample.to_le_bytes(), sample.to_le_bytes()].concat()
        })
        .collect();
    let mut file = Vec::with_capacity(44 + samples.len());
    file.extend_from_slice(b"RIFF");
    file.extend_from_slice(&(36 + samples.len() as u32).to_le_bytes());
    file.extend_from_slice(b"WAVEfmt ");
    file.extend_from_slice(&16_u32.to_le_bytes());
    // Whole-number samples, two channels, the rate, bytes a second, bytes a frame, bits.
    file.extend_from_slice(&1_u16.to_le_bytes());
    file.extend_from_slice(&2_u16.to_le_bytes());
    file.extend_from_slice(&RATE.to_le_bytes());
    file.extend_from_slice(&(RATE * 4).to_le_bytes());
    file.extend_from_slice(&4_u16.to_le_bytes());
    file.extend_from_slice(&16_u16.to_le_bytes());
    file.extend_from_slice(b"data");
    file.extend_from_slice(&(samples.len() as u32).to_le_bytes());
    file.extend_from_slice(&samples);
    std::fs::write(path, file).expect("the tone is written");
}

const COMPOSITION: &str = r#"<composition width="64" height="32" fps="30" duration="1">
  <video id="clip" src="clip.mp4" left="0" top="0" width="64" height="32" start="0" duration="1"/>
  <audio id="tone" src="tone.wav" start="0" duration="1"/>
</composition>"#;

/// Renders the composition of `folder` with nothing on the search path and returns the file.
fn rendered(folder: &Path, options: &[&str]) -> PathBuf {
    let nothing = folder.join("no-programs");
    std::fs::create_dir_all(&nothing).expect("the empty folder is made");
    let output = folder.join("out.mp4");
    let ran = Command::new(env!("CARGO_BIN_EXE_poc"))
        .arg("render")
        .arg(folder.join("composition.xml"))
        .arg("--output")
        .arg(&output)
        .args(options)
        .env("PATH", &nothing)
        .output()
        .expect("poc runs");
    assert!(ran.status.success(), "poc render: {}", String::from_utf8_lossy(&ran.stderr));
    output
}

/// Checks that the file holds the clip's pictures, frame for frame, in `codec`, and the tone,
/// whole.
fn holds_the_film(output: &Path, codec: Codec) {
    let (formats, codecs) = registries();
    let demuxer = formats.open(output).expect("the finished file opens");
    let tracks = demuxer.tracks();
    assert_eq!(tracks.len(), 2, "a picture track and a sound track");
    let picture = tracks
        .iter()
        .find(|track| track.video().is_some())
        .expect("the file holds pictures");
    assert_eq!((picture.codec, picture.samples), (codec, u64::from(FRAMES)));
    let sound = tracks.iter().find(|track| track.audio().is_some()).expect("the file holds sound");
    assert_eq!(sound.codec, Codec::Aac);
    assert_eq!(
        (sound.duration, sound.timescale),
        (u64::from(RATE), RATE),
        "the sound is as long as the film, to the sample"
    );

    let pace = Pace {
        size: (WIDTH, HEIGHT),
        rate: Rational::new(30, 1),
        first: 0,
        form: Form::Planes,
    };
    let mut footage = Footage::start(output, &pace, &formats, &codecs).expect("the finished file is decoded");
    let mut planes = vec![0; Form::Planes.bytes((WIDTH, HEIGHT))];
    let centre = (HEIGHT as usize / 2) * WIDTH as usize + WIDTH as usize / 2;
    for frame in 0..FRAMES {
        assert!(
            footage.read(u64::from(frame), &mut planes).expect("a frame is read"),
            "frame {frame} is there"
        );
        let found = planes[centre];
        assert!(
            found.abs_diff(luma(frame)) <= 2,
            "frame {frame} has luma {found}; its picture of the clip has {}",
            luma(frame)
        );
    }

    let mut heard = codecs.audio_source(output).expect("the sound of the finished file is decoded");
    assert_eq!((heard.sample_rate(), heard.channels()), (RATE, 2));
    let mut samples = vec![0.0_f32; RATE as usize * 2 * 2];
    let read = heard.read(&mut samples).expect("the sound is read");
    assert!(read >= RATE as usize, "a second of sound, and {read} frames were read");
    // The middle half of the second, away from the encoder's first and last packets.
    let middle = &samples[RATE as usize / 2..RATE as usize * 3 / 2];
    let highest = middle.iter().fold(0.0_f32, |highest, sample| highest.max(sample.abs()));
    assert!((highest - TONE).abs() < 0.03, "the tone reaches {highest}, not {TONE}");
}

#[test]
fn a_film_with_footage_and_sound_is_rendered_with_no_other_program_to_be_found() {
    let folder = folder("one-worker");
    clip(&folder.join("clip.mp4"));
    tone(&folder.join("tone.wav"));
    std::fs::write(folder.join("composition.xml"), COMPOSITION).expect("the composition is written");
    holds_the_film(&rendered(&folder, &["--backend", "cpu"]), Codec::H264);
}

/// This test needs a graphics adapter.
#[cfg(feature = "gpu")]
#[test]
fn the_gpu_renderer_needs_no_other_program_either() {
    let folder = folder("gpu");
    clip(&folder.join("clip.mp4"));
    tone(&folder.join("tone.wav"));
    std::fs::write(folder.join("composition.xml"), COMPOSITION).expect("the composition is written");
    holds_the_film(&rendered(&folder, &["--backend", "gpu"]), Codec::H264);
}

#[test]
fn the_software_encoder_is_in_the_program_too() {
    let folder = folder("software");
    clip(&folder.join("clip.mp4"));
    tone(&folder.join("tone.wav"));
    std::fs::write(folder.join("composition.xml"), COMPOSITION).expect("the composition is written");
    let output = rendered(&folder, &["--backend", "cpu", "--encoder", "software"]);
    let (formats, _) = registries();
    let demuxer = formats.open(&output).expect("the finished file opens");
    let pictures = demuxer
        .tracks()
        .iter()
        .find(|track| track.video().is_some())
        .expect("the file holds pictures");
    assert_eq!((pictures.codec, pictures.samples), (Codec::H264, u64::from(FRAMES)));
}

#[test]
fn a_film_is_hevc_when_that_codec_is_asked_for() {
    let folder = folder("hevc");
    clip(&folder.join("clip.mp4"));
    tone(&folder.join("tone.wav"));
    std::fs::write(folder.join("composition.xml"), COMPOSITION).expect("the composition is written");
    holds_the_film(&rendered(&folder, &["--backend", "cpu", "--codec", "hevc"]), Codec::Hevc);
}

/// A film of 60 frames a second from a clip of 30: twice the pictures, each lasting half as
/// long, and each picture of the clip shown twice.
#[test]
fn a_film_of_sixty_frames_a_second_holds_sixty_pictures_for_each_second() {
    let folder = folder("sixty");
    clip(&folder.join("clip.mp4"));
    tone(&folder.join("tone.wav"));
    let composition = COMPOSITION.replace(r#"fps="30""#, r#"fps="60""#);
    std::fs::write(folder.join("composition.xml"), composition).expect("the composition is written");
    let output = rendered(&folder, &["--backend", "cpu"]);

    let (formats, codecs) = registries();
    let demuxer = formats.open(&output).expect("the finished file opens");
    let track = demuxer
        .tracks()
        .iter()
        .find(|track| track.video().is_some())
        .expect("the file holds pictures");
    let pictures = track.video().expect("the track holds pictures");
    assert_eq!((track.samples, pictures.rate, pictures.steady), (60, Rational::new(60, 1), true));
    assert_eq!(track.duration, u64::from(track.timescale), "one second of pictures");
    let sound = demuxer
        .tracks()
        .iter()
        .find(|track| track.audio().is_some())
        .expect("the file holds sound");
    assert_eq!((sound.duration, sound.timescale), (u64::from(RATE), RATE));

    let pace = Pace {
        size: (WIDTH, HEIGHT),
        rate: Rational::new(60, 1),
        first: 0,
        form: Form::Planes,
    };
    let mut footage = Footage::start(&output, &pace, &formats, &codecs).expect("the finished file is decoded");
    let mut planes = vec![0; Form::Planes.bytes((WIDTH, HEIGHT))];
    let centre = (HEIGHT as usize / 2) * WIDTH as usize + WIDTH as usize / 2;
    for frame in 0..60 {
        assert!(footage.read(frame, &mut planes).expect("a frame is read"), "frame {frame} is there");
        let wanted = luma(frame as u32 / 2);
        assert!(
            planes[centre].abs_diff(wanted) <= 2,
            "frame {frame} has luma {}; its picture of the clip has {wanted}",
            planes[centre]
        );
    }
}

/// A film of 8K: this machine's hardware does not encode H.264 that large, and says which
/// setting to change; as HEVC the film is rendered. This test needs a graphics adapter and
/// hardware that encodes HEVC at 8K.
#[cfg(feature = "gpu")]
#[test]
fn a_film_too_large_for_h264_on_hardware_is_refused_by_name_and_rendered_as_hevc() {
    let folder = folder("large");
    let composition = r##"<composition width="7680" height="4320" fps="30" duration="1">
  <rect id="plate" left="0" top="0" width="7680" height="4320" background="#204060"/>
  <rect id="dot" left="0" top="2000" width="320" height="320" background="#ffcc00">
    <tween at="0" duration="1" from-x="0" to-x="7360"/>
  </rect>
</composition>"##;
    std::fs::write(folder.join("composition.xml"), composition).expect("the composition is written");
    let output = folder.join("out.mp4");
    let render = |options: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_poc"))
            .arg("render")
            .arg(folder.join("composition.xml"))
            .args(["--backend", "gpu", "--frames", "0:4", "--output"])
            .arg(&output)
            .args(options)
            .output()
            .expect("poc runs")
    };
    let refused = render(&[]);
    let said = String::from_utf8_lossy(&refused.stderr).to_string();
    assert!(!refused.status.success(), "H.264 at 8K is refused on hardware");
    assert!(said.contains("7680 by 4320") && said.contains("codec: hevc"), "{said}");
    assert!(!output.exists(), "a render that was refused leaves no file");

    let ran = render(&["--codec", "hevc"]);
    assert!(ran.status.success(), "poc render: {}", String::from_utf8_lossy(&ran.stderr));
    let (formats, _) = registries();
    let demuxer = formats.open(&output).expect("the finished file opens");
    let track = &demuxer.tracks()[0];
    let pictures = track.video().expect("the file holds pictures");
    assert_eq!((track.codec, track.samples), (Codec::Hevc, 4));
    assert_eq!((pictures.width, pictures.height), (7680, 4320));
}
