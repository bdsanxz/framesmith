//! Sprint 12 T004 end-to-end: carry HDR end to end.
//!
//! A PQ source with `--hdr auto` renders HEVC Main10 BT.2020 (per
//! probe); an SDR-only composition renders SDR; explicit `pq`/`hlg`
//! demand their transfer; an HDR source to GIF renders a tone-mapped
//! SDR animation with the mapping named in the report; explicit HDR
//! with SDR-only sources, and HDR with a format that cannot carry it,
//! refuse naming the cause with a concrete fix before frame one,
//! leaving no partial output behind.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n12t004-{tag}-{}-{}",
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

fn ffprobe(args: &[&str], path: &Path) -> String {
    let out = Command::new("ffprobe")
        .args(["-v", "error"])
        .args(args)
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

/// One video-stream entry of a delivered file.
fn stream_entries(path: &Path) -> String {
    ffprobe(
        &[
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name,pix_fmt,color_transfer,color_primaries,color_space",
            "-of",
            "default=noprint_wrappers=1",
        ],
        path,
    )
}

/// Generate a small HDR (PQ or HLG) HEVC source inside `folder`.
fn gen_hdr(folder: &Path, name: &str, transfer: &str) {
    let out = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=s=160x120:r=15:d=1",
            "-c:v",
            "libx265",
            "-pix_fmt",
            "yuv420p10le",
            "-x265-params",
            &format!("colorprim=bt2020:transfer={transfer}:colormatrix=bt2020nc"),
            "-t",
            "1",
        ])
        .arg(folder.join(name))
        .output()
        .expect("generate HDR footage");
    assert!(
        out.status.success(),
        "host ffmpeg HDR generation failed for {transfer}"
    );
}

/// Project whose only picture is one footage clip (plus silence).
fn footage_project(root: &Path, name: &str, source: &str) -> PathBuf {
    let folder = root.join(name);
    std::fs::create_dir_all(&folder).unwrap();
    gen_hdr(
        &folder,
        source,
        if source.contains("hlg") {
            "arib-std-b67"
        } else {
            "smpte2084"
        },
    );
    write(
        &folder,
        "composition.kdl",
        &format!(
            "composition name=\"t\" width=160 height=120 duration=1 fps=15 background=\"#112233\" version=1 {{\n    video path=\"{source}\" start=0 duration=1 fit=\"fill\"\n}}\n"
        ),
    );
    folder
}

/// Text-only SDR project: no footage, so no source carries HDR.
fn sdr_project(root: &Path) -> PathBuf {
    let folder = root.join("sproj");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=160 height=120 duration=1 fps=15 background=\"#112233\" version=1 {\n    text value=\"Hi\" start=0 duration=1 x=10 y=20 size=24 color=\"#ffffff\"\n}\n",
    );
    folder
}

#[test]
fn auto_pq_source_renders_hevc_main10_bt2020() {
    let root = tmp("auto-pq");
    let folder = footage_project(&root, "proj", "hdr-pq.mp4");
    let (code, out) = run(
        &["render", "proj", "--hdr", "auto", "-o", "out-hdr.mp4"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let target = root.join("out-hdr.mp4");
    assert!(target.is_file(), "HDR output written: {out}");
    let probe = stream_entries(&target);
    assert!(probe.contains("codec_name=hevc"), "{probe}");
    assert!(probe.contains("pix_fmt=yuv420p10le"), "{probe}");
    assert!(probe.contains("color_transfer=smpte2084"), "{probe}");
    assert!(probe.contains("color_primaries=bt2020"), "{probe}");
    assert!(probe.contains("color_space=bt2020nc"), "{probe}");
    // The report names the decision the probe drove.
    assert!(out.contains("smpte2084"), "{out}");
    assert!(out.contains("Main10"), "{out}");
    assert!(out.contains("hdr-pq.mp4"), "{out}");
    // The default `--format mp4` carries HEVC Main10 when HDR is
    // active (not the default H.264).
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--format",
            "hevc",
            "--hdr",
            "auto",
            "-o",
            "out-hevc.mp4",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let probe = stream_entries(&root.join("out-hevc.mp4"));
    assert!(probe.contains("codec_name=hevc"), "{probe}");
    assert!(probe.contains("color_transfer=smpte2084"), "{probe}");
    // A re-render on software is byte-identical.
    let (code, out) = run(
        &["render", "proj", "--hdr", "auto", "-o", "out-hdr2.mp4"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let a = std::fs::read(root.join("out-hdr.mp4")).unwrap();
    let b = std::fs::read(root.join("out-hdr2.mp4")).unwrap();
    assert_eq!(a, b, "software HDR re-render is byte-identical");
}

#[test]
fn auto_sdr_composition_renders_sdr() {
    let root = tmp("auto-sdr");
    let _folder = sdr_project(&root);
    let (code, out) = run(&["render", "sproj", "-o", "out.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let probe = stream_entries(&root.join("out.mp4"));
    assert!(probe.contains("codec_name=h264"), "{probe}");
    assert!(probe.contains("pix_fmt=yuv420p"), "{probe}");
    assert!(!probe.contains("smpte2084"), "{probe}");
    assert!(!probe.contains("arib-std-b67"), "{probe}");
    // A bare `auto` resolving to SDR stays silent: no HDR note rides
    // the pinned lines.
    assert!(!out.contains("transfer="), "{out}");
    assert!(!out.contains("Main10"), "{out}");
    // `--hdr off` on SDR matches the default bytes exactly.
    let (code, out) = run(
        &["render", "sproj", "--hdr", "off", "-o", "out-off.mp4"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let a = std::fs::read(root.join("out.mp4")).unwrap();
    let b = std::fs::read(root.join("out-off.mp4")).unwrap();
    assert_eq!(a, b, "SDR `--hdr off` matches the default bytes");
}

#[test]
fn explicit_modes_render_their_demanded_transfer() {
    let root = tmp("explicit");
    let folder = footage_project(&root, "pproj", "hdr-pq.mp4");
    gen_hdr(&folder, "hdr-hlg.mp4", "arib-std-b67");
    // Explicit `pq` with a PQ source renders PQ metadata.
    let (code, out) = run(&["render", "pproj", "--hdr", "pq", "-o", "pq.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let probe = stream_entries(&root.join("pq.mp4"));
    assert!(probe.contains("color_transfer=smpte2084"), "{probe}");
    assert!(probe.contains("pix_fmt=yuv420p10le"), "{probe}");
    assert!(out.contains("matched PQ"), "{out}");
    // Explicit `hlg` with an HLG source renders HLG metadata. The
    // project also holds the PQ clip, so point at the HLG one via a
    // second composition.
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" root=true width=160 height=120 duration=1 fps=15 background=\"#112233\" version=1 {\n    video path=\"hdr-pq.mp4\" start=0 duration=1 fit=\"fill\"\n}\ncomposition name=\"h\" width=160 height=120 duration=1 fps=15 background=\"#112233\" version=1 {\n    video path=\"hdr-hlg.mp4\" start=0 duration=1 fit=\"fill\"\n}\n",
    );
    let (code, out) = run(
        &[
            "render",
            "pproj",
            "--composition",
            "h",
            "--hdr",
            "hlg",
            "-o",
            "hlg.mp4",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let probe = stream_entries(&root.join("hlg.mp4"));
    assert!(probe.contains("color_transfer=arib-std-b67"), "{probe}");
    assert!(probe.contains("pix_fmt=yuv420p10le"), "{probe}");
}

#[test]
fn hdr_source_to_gif_renders_tone_mapped_sdr() {
    let root = tmp("tonemap");
    let _folder = footage_project(&root, "proj", "hdr-pq.mp4");
    let (code, out) = run(
        &["render", "proj", "--format", "gif", "-o", "out.gif"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let target = root.join("out.gif");
    assert!(target.is_file(), "tone-mapped GIF written: {out}");
    // The report names the mapping the probe drove.
    assert!(out.contains("tone-mapped"), "{out}");
    assert!(out.contains("smpte2084"), "{out}");
    assert!(out.contains("--format gif"), "{out}");
    // The animation is SDR by nature and covers every frame.
    let frames: u64 = ffprobe(
        &[
            "-count_frames",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=nb_read_frames",
            "-of",
            "csv=p=0",
        ],
        &target,
    )
    .trim()
    .parse()
    .expect("parse gif frame count");
    assert_eq!(frames, 15, "gif covers the full composition");
    // PNG sequences tone-map the same way.
    let (code, out) = run(
        &["render", "proj", "--format", "pngseq", "-o", "stills"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("tone-mapped"), "{out}");
    let count = std::fs::read_dir(root.join("stills")).unwrap().count();
    assert_eq!(count, 15, "pngseq covers every frame");
}

#[test]
fn explicit_hdr_with_sdr_sources_refuses() {
    let root = tmp("refuse-sdr");
    let _folder = sdr_project(&root);
    let (code, out) = run(&["render", "sproj", "--hdr", "pq", "-o", "out.mp4"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--hdr pq"), "{out}");
    assert!(out.contains("SDR"), "{out}");
    assert!(out.contains("--hdr auto"), "{out}");
    assert!(out.contains("no output was written"), "{out}");
    assert!(
        !root.join("out.mp4").exists(),
        "refusal leaves no partial output"
    );
    // Demanding HLG from a PQ-only composition refuses the same way.
    let root = tmp("refuse-wrong");
    let _folder = footage_project(&root, "proj", "hdr-pq.mp4");
    let (code, out) = run(&["render", "proj", "--hdr", "hlg", "-o", "out.mp4"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--hdr hlg"), "{out}");
    assert!(out.contains("arib-std-b67"), "{out}");
    assert!(out.contains("no output was written"), "{out}");
    assert!(!root.join("out.mp4").exists());
}

#[test]
fn hdr_with_uncarrying_format_refuses() {
    let root = tmp("refuse-fmt");
    let _folder = footage_project(&root, "proj", "hdr-pq.mp4");
    // Explicit HDR to GIF names the format limit with a fix.
    let (code, out) = run(
        &[
            "render", "proj", "--format", "gif", "--hdr", "pq", "-o", "out.gif",
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--format gif"), "{out}");
    assert!(out.contains("cannot carry HDR"), "{out}");
    assert!(out.contains("--format hevc"), "{out}");
    assert!(out.contains("no output was written"), "{out}");
    assert!(!root.join("out.gif").exists());
    // `auto` with an HDR source to WebM refuses the same limit (only
    // GIF/PNG sequences tone-map).
    let (code, out) = run(
        &["render", "proj", "--format", "webm", "-o", "out.webm"],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--format webm"), "{out}");
    assert!(out.contains("cannot carry HDR"), "{out}");
    assert!(!root.join("out.webm").exists());
}

#[test]
fn bad_hdr_flag_values_refuse_before_frame_one() {
    let root = tmp("bad-hdr");
    let _folder = sdr_project(&root);
    let (code, out) = run(
        &["render", "sproj", "--hdr", "dolby", "-o", "out.mp4"],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--hdr"), "{out}");
    assert!(out.contains("off, pq, hlg, or auto"), "{out}");
    assert!(!root.join("out.mp4").exists());
    let (code, _) = run(
        &[
            "render", "sproj", "--hdr", "pq", "--hdr", "hlg", "-o", "out.mp4",
        ],
        &root,
    );
    assert_eq!(code, 2, "duplicate --hdr exits 2");
}

#[test]
fn json_report_names_the_hdr_decision() {
    let root = tmp("json-hdr");
    let _folder = footage_project(&root, "proj", "hdr-pq.mp4");
    let (code, out) = run(
        &["render", "proj", "--hdr", "auto", "--json", "-o", "out.mp4"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"hdr\":\"auto\""), "{out}");
    assert!(out.contains("\"transfer\":\"pq\""), "{out}");
    let _folder = sdr_project(&root);
    let (code, out) = run(&["render", "sproj", "--json", "-o", "s.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"transfer\":\"sdr\""), "{out}");
}
