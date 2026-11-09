//! Sprint 12 T001 end-to-end: deliver many formats.
//!
//! One composition renders to all seven `--format` destinations with the
//! requested codec, size, and pictures; ProRes honors every profile;
//! GIF loops deterministically; PNG sequences number exactly;
//! HLS writes a playlist plus decodable segments; bare `mp4` stays
//! byte-identical; and every refusal names the flag with a concrete
//! fix before frame one, leaving no partial output behind.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n12t001-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_framesmith"))
}

fn run(args: &[&str], cwd: &Path) -> (i32, String) {
    let out = Command::new(bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run framesmith binary");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

fn ffmpeg(args: &[&str]) {
    let out = Command::new("ffmpeg")
        .args(["-y", "-v", "error"])
        .args(args)
        .output()
        .expect("run host ffmpeg");
    assert!(out.status.success(), "host ffmpeg failed: {args:?}");
}

fn gen_tone(path: &Path) {
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:duration=1",
        "-ar",
        "48000",
        "-ac",
        "2",
        &path.display().to_string(),
    ]);
}

fn ffprobe_show(path: &Path, entries: &str) -> String {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            entries,
            "-of",
            "default=noprint_wrappers=1",
        ])
        .arg(path)
        .output()
        .expect("run host ffprobe");
    assert!(
        out.status.success(),
        "ffprobe failed for {}",
        path.display()
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn streams(path: &Path) -> String {
    ffprobe_show(
        path,
        "stream=codec_name,width,height,pix_fmt,sample_rate,channels",
    )
}

/// Human composition used by every destination: text over color plus
/// an audible tone, so audio-carrying formats exercise the mix and
/// silent-picture formats prove they ignore it.
fn project(root: &Path) -> PathBuf {
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_tone(&folder.join("tone.wav"));
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=160 height=120 duration=1 fps=15 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=20 size=24 color=\"#ffffff\"\n    audio path=\"tone.wav\" start=0 duration=1\n}\n",
    );
    folder
}

/// Decoded RGB24 frames of one video file.
fn decode_rgb(path: &Path, frames: usize, w: u32, h: u32) -> Vec<u8> {
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            &path.display().to_string(),
            "-frames:v",
            &frames.to_string(),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .output()
        .expect("decode video");
    assert!(out.status.success(), "decode failed for {}", path.display());
    assert_eq!(out.stdout.len(), frames * w as usize * h as usize * 3);
    out.stdout
}

fn marked_pixels(raw: &[u8], bg: (u8, u8, u8)) -> usize {
    raw.as_chunks::<3>()
        .0
        .iter()
        .filter(|p| {
            (p[0] as i32 - bg.0 as i32).abs()
                + (p[1] as i32 - bg.1 as i32).abs()
                + (p[2] as i32 - bg.2 as i32)
                > 100
        })
        .count()
}

#[test]
fn each_format_renders_and_probes_to_its_codec() {
    let root = tmp("codecs");
    let folder = project(&root);
    // (format, extra flags, output file, video codec, audio codec, pix fmt)
    type Case<'a> = (
        &'a str,
        &'a [&'a str],
        &'a str,
        &'a str,
        Option<&'a str>,
        &'a str,
    );
    let cases: &[Case] = &[
        ("mp4", &[], "out.mp4", "h264", Some("aac"), "yuv420p"),
        ("hevc", &[], "out-hevc.mp4", "hevc", Some("aac"), "yuv420p"),
        ("webm", &[], "out.webm", "vp9", Some("opus"), "yuv420p"),
        (
            "prores",
            &["--prores-profile", "hq"],
            "out.mov",
            "prores",
            Some("pcm_s16le"),
            "yuv422p10le",
        ),
        ("gif", &[], "out.gif", "gif", None, "rgb24"),
        ("hls", &[], "stream.m3u8", "h264", Some("aac"), "yuv420p"),
    ];
    for (format, extra, name, vcodec, acodec, pix) in cases {
        let mut args: Vec<&str> = vec!["render", "proj", "--format", format];
        args.extend_from_slice(extra);
        args.extend_from_slice(&["-o", name]);
        let (code, out) = run(&args, &root);
        assert_eq!(code, 0, "render --format {format}: {out}");
        let target = root.join(name);
        if *format == "hls" {
            // The playlist resolves beside its segments.
            assert!(target.is_file(), "playlist written: {out}");
            let body = std::fs::read_to_string(&target).unwrap();
            assert!(body.contains("#EXTM3U"), "{body}");
            let segs: Vec<String> = body
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(str::to_string)
                .collect();
            assert!(!segs.is_empty(), "playlist names segments: {body}");
            assert!(segs.len() <= 2, "one segment per 2s window: {body}");
            for seg in &segs {
                let seg_path = root.join(seg);
                let probe = streams(&seg_path);
                assert!(probe.contains("codec_name=h264"), "{probe}");
                if let Some(a) = acodec {
                    assert!(probe.contains(&format!("codec_name={a}")), "{probe}");
                }
            }
            continue;
        }
        let probe = streams(&target);
        assert!(
            probe.contains(&format!("codec_name={vcodec}")),
            "format {format}: {probe}"
        );
        assert!(probe.contains("width=160"), "format {format}: {probe}");
        assert!(probe.contains("height=120"), "format {format}: {probe}");
        if *format != "gif" {
            assert!(
                probe.contains(&format!("pix_fmt={pix}")),
                "format {format}: {probe}"
            );
        }
        match acodec {
            Some(a) => assert!(
                probe.contains(&format!("codec_name={a}")),
                "format {format}: {probe}"
            ),
            // GIF carries no audio stream at all.
            None => assert!(!probe.contains("sample_rate"), "gif is silent: {probe}"),
        }
        // The text mark survives every codec roundtrip.
        let raw = decode_rgb(&target, 1, 160, 120);
        assert!(
            marked_pixels(&raw, (0x11, 0x22, 0x33)) > 50,
            "text drawn in {format}"
        );
    }
    // PNG sequences number every frame exactly.
    let (code, out) = run(
        &["render", "proj", "--format", "pngseq", "-o", "stills"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let dir = root.join("stills");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let expected: Vec<String> = (0..15).map(|i| format!("frame-{i:04}.png")).collect();
    assert_eq!(names, expected);
    let probe = streams(&dir.join("frame-0000.png"));
    assert!(probe.contains("codec_name=png"), "{probe}");
    assert!(probe.contains("width=160"), "{probe}");
    let raw = decode_rgb(&dir.join("frame-0007.png"), 1, 160, 120);
    assert!(marked_pixels(&raw, (0x11, 0x22, 0x33)) > 50);
    // Default output names resolve inside the project folder.
    let (code, out) = run(&["render", "proj", "--format", "webm"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(folder.join("out.webm").is_file());
    let (code, out) = run(&["render", "proj", "--format", "pngseq"], &root);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        std::fs::read_dir(folder.join("frames")).unwrap().count(),
        15
    );
    let (code, out) = run(&["render", "proj", "--format", "hls"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(folder.join("hls").join("index.m3u8").is_file());
}

/// Mean per-channel absolute difference between two raw frames.
fn mean_abs_diff(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    let total: u64 = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| (*x as i32 - *y as i32).unsigned_abs() as u64)
        .sum();
    total as f64 / a.len() as f64
}

#[test]
fn webm_composites_alpha_like_the_mp4() {
    let root = tmp("alpha");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    // An alpha-carrying video source: PNG pictures with a real alpha
    // plane in a MOV (ffprobe reports `pix_fmt=rgba` with exact timing).
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "nullsrc=size=64x48:rate=15",
        "-vf",
        "format=rgba,geq=r=255:g=0:b=0:a=128",
        "-t",
        "1",
        "-r",
        "15",
        "-c:v",
        "png",
        "-pix_fmt",
        "rgba",
        &folder.join("alpha.mov").display().to_string(),
    ]);
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=160 height=120 duration=1 fps=15 background=\"#112233\" version=1 {\n    video path=\"alpha.mov\" start=0 duration=1 fit=\"fill\"\n}\n",
    );
    // Alpha composites at draw time, so the WebM carries the same
    // pictures as the MP4; the container stays opaque because this
    // stack's libvpx drops alpha planes even when requested.
    let (code, out) = run(
        &["render", "proj", "--format", "webm", "-o", "a.webm"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "proj", "-o", "a.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let probe = streams(&root.join("a.webm"));
    assert!(probe.contains("codec_name=vp9"), "{probe}");
    let (w, r) = (
        decode_rgb(&root.join("a.webm"), 1, 160, 120),
        decode_rgb(&root.join("a.mp4"), 1, 160, 120),
    );
    assert!(
        mean_abs_diff(&w, &r) < 12.0,
        "webm composites alpha like the mp4"
    );
}

#[test]
fn stills_with_alpha_composite_like_the_mp4() {
    let root = tmp("still-alpha");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    // Half-transparent red still: decoded RGBA carries alpha < 255.
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "nullsrc=size=32x32:rate=1",
        "-vf",
        "format=rgba,geq=r=255:g=0:b=0:a=128",
        "-frames:v",
        "1",
        "-pix_fmt",
        "rgba",
        &folder.join("half.png").display().to_string(),
    ]);
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=160 height=120 duration=1 fps=15 background=\"#112233\" version=1 {\n    image path=\"half.png\" start=0 duration=1 x=10 y=10 width=50 height=50 fit=\"contain\" placement=\"top-left\" order=1\n}\n",
    );
    // Like video alpha, still alpha composites at draw time; the WebM
    // carries the same pictures as the MP4.
    let (code, out) = run(
        &["render", "proj", "--format", "webm", "-o", "s.webm"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "proj", "-o", "s.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let (w, r) = (
        decode_rgb(&root.join("s.webm"), 1, 160, 120),
        decode_rgb(&root.join("s.mp4"), 1, 160, 120),
    );
    assert!(
        mean_abs_diff(&w, &r) < 12.0,
        "webm composites still alpha like the mp4"
    );
}

#[test]
fn prores_defaults_to_standard_and_honors_each_profile() {
    let root = tmp("prores");
    project(&root);
    for (profile, pix) in [
        ("proxy", "yuv422p10le"),
        ("lt", "yuv422p10le"),
        ("standard", "yuv422p10le"),
        ("hq", "yuv422p10le"),
        ("4444", "yuv444p12le"),
    ] {
        let name = format!("out-{profile}.mov");
        let (code, out) = run(
            &[
                "render",
                "proj",
                "--format",
                "prores",
                "--prores-profile",
                profile,
                "-o",
                &name,
            ],
            &root,
        );
        assert_eq!(code, 0, "prores {profile}: {out}");
        assert!(out.contains(&format!("profile={profile}")), "{out}");
        let probe = streams(&root.join(&name));
        assert!(probe.contains("codec_name=prores"), "{probe}");
        assert!(probe.contains(&format!("pix_fmt={pix}")), "{probe}");
    }
    // No profile flag means `standard`.
    let (code, out) = run(
        &["render", "proj", "--format", "prores", "-o", "d.mov"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("profile=standard"), "{out}");
}

#[test]
fn gif_loops_the_full_composition_deterministically() {
    let root = tmp("gif");
    project(&root);
    let run_gif = |name: &str| run(&["render", "proj", "--format", "gif", "-o", name], &root);
    let (code, out) = run_gif("a.gif");
    assert_eq!(code, 0, "{out}");
    let (code, out) = run_gif("b.gif");
    assert_eq!(code, 0, "{out}");
    let (a, b) = (
        std::fs::read(root.join("a.gif")).unwrap(),
        std::fs::read(root.join("b.gif")).unwrap(),
    );
    assert_eq!(a, b, "gif re-renders byte-identically");
    // The Netscape looping extension repeats the animation forever.
    assert!(
        a.windows(b"NETSCAPE2.0".len()).any(|w| w == b"NETSCAPE2.0"),
        "gif loops"
    );
    // Every composition frame decodes.
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            &root.join("a.gif").display().to_string(),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-",
        ])
        .output()
        .expect("decode gif");
    assert!(out.status.success());
    assert_eq!(out.stdout.len(), 15 * 160 * 120 * 3);
}

#[test]
fn pngseq_reruns_frame_identically() {
    let root = tmp("pngseq-det");
    project(&root);
    let (code, out) = run(&["render", "proj", "--format", "pngseq", "-o", "s1"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "proj", "--format", "pngseq", "-o", "s2"], &root);
    assert_eq!(code, 0, "{out}");
    for i in [0, 7, 14] {
        let (a, b) = (
            std::fs::read(root.join(format!("s1/frame-{i:04}.png"))).unwrap(),
            std::fs::read(root.join(format!("s2/frame-{i:04}.png"))).unwrap(),
        );
        assert_eq!(a, b, "frame {i} re-renders byte-identically");
    }
}

#[test]
fn software_reruns_stay_byte_identical_per_format() {
    let root = tmp("determinism");
    project(&root);
    // HLS compares the playlist plus its segments.
    for dir in ["h1", "h2"] {
        let (code, out) = run(&["render", "proj", "--format", "hls", "-o", dir], &root);
        assert_eq!(code, 0, "{out}");
    }
    let (a, b) = (
        std::fs::read(root.join("h1/index.m3u8")).unwrap(),
        std::fs::read(root.join("h2/index.m3u8")).unwrap(),
    );
    assert_eq!(a, b, "hls playlist re-renders byte-identically");
    let seg_a = std::fs::read(root.join("h1/seg-000.ts")).unwrap();
    let seg_b = std::fs::read(root.join("h2/seg-000.ts")).unwrap();
    assert_eq!(seg_a, seg_b, "hls segments re-render byte-identically");
    let once: &[(&str, &[&str], &str)] = &[
        ("mp4", &[], "a.mp4"),
        ("hevc", &[], "a-hevc.mp4"),
        ("webm", &[], "a.webm"),
        ("prores", &["--prores-profile", "hq"], "a.mov"),
    ];
    for (format, extra, name) in once {
        let mut args: Vec<&str> = vec!["render", "proj", "--format", format];
        args.extend_from_slice(extra);
        args.extend_from_slice(&["-o", name]);
        let (code, out) = run(&args, &root);
        assert_eq!(code, 0, "first {format}: {out}");
        let again = format!("again-{name}");
        let mut args: Vec<&str> = vec!["render", "proj", "--format", format];
        args.extend_from_slice(extra);
        args.extend_from_slice(&["-o", &again]);
        let (code, out) = run(&args, &root);
        assert_eq!(code, 0, "second {format}: {out}");
        let (a, b) = (
            std::fs::read(root.join(name)).unwrap(),
            std::fs::read(root.join(&again)).unwrap(),
        );
        assert_eq!(a, b, "{format} re-renders byte-identically");
    }
}

#[test]
fn bare_mp4_renders_byte_identical_to_before() {
    let root = tmp("bare-mp4");
    project(&root);
    let (code, out) = run(&["render", "proj", "-o", "a.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "proj", "-o", "b.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        std::fs::read(root.join("a.mp4")).unwrap(),
        std::fs::read(root.join("b.mp4")).unwrap()
    );
    // The default output name is unchanged.
    let (code, out) = run(&["render", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(root.join("proj").join("out.mp4").is_file());
}

#[test]
fn preset_and_bitrate_axes_ride_along_on_other_formats() {
    let root = tmp("flags-combine");
    project(&root);
    // Draft on HEVC sets its own axis with x265-compatible flags.
    let (code, out) = run(
        &[
            "render", "proj", "--format", "hevc", "--preset", "draft", "-o", "d.mp4",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let probe = streams(&root.join("d.mp4"));
    assert!(probe.contains("codec_name=hevc"), "{probe}");
    // Draft on GIF renders (GIF has no CRF axis to set).
    let (code, out) = run(
        &[
            "render", "proj", "--format", "gif", "--preset", "draft", "-o", "d.gif",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    // A bitrate cap rides along on a ProRes MOV.
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--format",
            "prores",
            "--video-bitrate",
            "20M",
            "-o",
            "c.mov",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let probe = streams(&root.join("c.mov"));
    assert!(probe.contains("codec_name=prores"), "{probe}");
}

#[test]
fn json_report_names_the_destination_format() {
    let root = tmp("json-format");
    project(&root);
    let out = Command::new(bin())
        .args([
            "render", "proj", "--format", "hevc", "-o", "h.mp4", "--json",
        ])
        .current_dir(&root)
        .output()
        .expect("run framesmith binary");
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(stdout.contains("\"format\":\"hevc\""), "{stdout}");
    assert!(stdout.contains("\"output\":\"h.mp4\""), "{stdout}");
}

#[test]
fn unknown_format_fails_naming_the_accepted_values() {
    let root = tmp("bad-format");
    project(&root);
    let (code, out) = run(&["render", "proj", "--format", "avi", "-o", "x.mp4"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--format"), "{out}");
    assert!(
        out.contains("mp4, hevc, webm, mov, prores, gif, pngseq, or hls"),
        "{out}"
    );
    assert!(!root.join("x.mp4").exists(), "nothing written");
    assert!(
        !root.join("x.mp4.framesmith-part").exists(),
        "no staged file"
    );
}

#[test]
fn prores_profile_on_a_non_prores_format_fails() {
    let root = tmp("bad-profile");
    project(&root);
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--format",
            "webm",
            "--prores-profile",
            "hq",
            "-o",
            "x.webm",
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--prores-profile"), "{out}");
    assert!(out.contains("--format prores"), "{out}");
    assert!(!root.join("x.webm").exists(), "nothing written");
    // An unknown profile value names the accepted values.
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--format",
            "prores",
            "--prores-profile",
            "ultra",
            "-o",
            "x.mov",
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("proxy, lt, standard, hq, or 4444"), "{out}");
    assert!(!root.join("x.mov").exists(), "nothing written");
}

#[test]
fn unwritable_output_fails_before_frame_one() {
    let root = tmp("unwritable");
    project(&root);
    // A regular file blocks the output directory.
    std::fs::write(root.join("blocker"), b"no").unwrap();
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--format",
            "webm",
            "-o",
            "blocker/out.webm",
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("blocker"), "names the path: {out}");
    assert!(!root.join("blocker/out.webm").exists());
    // The same holds for a PNG directory destination.
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--format",
            "pngseq",
            "-o",
            "blocker/frames",
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("blocker/frames"), "names the path: {out}");
    assert!(!root.join("blocker/frames").exists(), "no partial dir");
    assert!(
        !root.join("blocker/frames.framesmith-part").exists(),
        "no staged dir"
    );
}

#[test]
fn vanished_source_stops_with_no_partial_file() {
    let root = tmp("vanished");
    let folder = project(&root);
    let (code, out) = run(&["render", "proj", "-o", "ok.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    std::fs::remove_file(folder.join("tone.wav")).unwrap();
    // The file destination names the path with no partial output.
    let (code, out) = run(&["render", "proj", "-o", "gone.mp4"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("tone.wav"), "names the path: {out}");
    assert!(!root.join("gone.mp4").exists(), "no partial file");
    assert!(
        !root.join("gone.mp4.framesmith-part").exists(),
        "no staged file"
    );
    // The sequence destination names the path with no partial dir.
    let (code, out) = run(
        &["render", "proj", "--format", "pngseq", "-o", "gone"],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("tone.wav"), "names the path: {out}");
    assert!(!root.join("gone").exists(), "no partial dir");
}

#[test]
fn hls_leaves_no_playlist_pointing_at_missing_segments() {
    let root = tmp("hls-verify");
    project(&root);
    let (code, out) = run(
        &["render", "proj", "--format", "hls", "-o", "stream"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let dir = root.join("stream");
    let body = std::fs::read_to_string(dir.join("index.m3u8")).unwrap();
    assert!(body.contains("#EXT-X-ENDLIST"), "{body}");
    for line in body.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        assert!(
            dir.join(line).is_file(),
            "playlist references an existing segment: {line}"
        );
        // Every segment decodes on its own.
        let probe = streams(&dir.join(line));
        assert!(probe.contains("codec_name=h264"), "{probe}");
    }
    assert!(out.contains("format=hls"), "{out}");
    assert!(out.contains("segment"), "{out}");
}
