//! Sprint 5 T006 (re-render unchanged footage faster from cache).
//!
//! Rendering a footage project twice with unchanged media bytes and
//! unchanged attributes reuses the decoded clip windows: the second
//! render is faster and byte-identical to the first, and the render
//! summary (human and `--json`) reports cache hits and misses. Editing
//! any media byte forces a fresh decode with no stale pictures. A
//! missing or corrupt cache degrades to a correct full render that
//! reports misses instead of hits. Output never depends on worker
//! count, and the cache stays bounded with correctness-safe eviction.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-footage-cache-{tag}-{}-{}",
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
    run_env(args, cwd, &[])
}

fn run_env(args: &[&str], cwd: &Path, env: &[(&str, &str)]) -> (i32, String) {
    let out = Command::new(bin())
        .args(args)
        .envs(env.iter().copied())
        .current_dir(cwd)
        .output()
        .expect("run framesmith binary");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

/// Stdout only (for `--json`: progress goes to stderr, stdout stays one
/// pure JSON document).
fn run_stdout(args: &[&str], cwd: &Path) -> (i32, String) {
    let out = Command::new(bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run framesmith binary");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

/// Re-render identity: whole-file bytes on Unix (deterministic
/// there), decoded-frame hashes on Windows (choco ffmpeg container
/// bytes vary run to run on identical pictures).
fn assert_identical_rerender(a: &Path, b: &Path, why: &str) {
    #[cfg(windows)]
    assert_same_picture(a, b, why);
    #[cfg(not(windows))]
    {
        assert_eq!(
            std::fs::read(a).unwrap(),
            std::fs::read(b).unwrap(),
            "{why}"
        );
    }
}

/// Decoded-frame identity between two renders: the framemd5
/// listing hashes every decoded frame, so container bytes cannot
/// hide a picture difference (and encoder noise cannot fake one).
fn assert_same_picture(a: &Path, b: &Path, why: &str) {
    let md5 = |f: &Path| -> String {
        let out = Command::new("ffmpeg")
            .args(["-nostdin", "-v", "error", "-i", &f.display().to_string(), "-f", "framemd5", "-"])
            .output()
            .expect("run host ffmpeg");
        assert!(out.status.success(), "framemd5 of {} failed", f.display());
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    assert_eq!(md5(a), md5(b), "{why}");
}

fn ffmpeg(args: &[&str]) {
    let out = Command::new("ffmpeg")
        .args(["-y", "-v", "error"])
        .args(args)
        .output()
        .expect("run host ffmpeg");
    assert!(out.status.success(), "host ffmpeg failed: {args:?}");
}

/// H.264 picture + PCM stereo sound, exactly measurable and loopable.
fn gen_clip(dir: &Path, name: &str, seconds: f64) {
    let tone = dir.join("tone.wav");
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        &format!("sine=frequency=440:duration={seconds}"),
        "-ar",
        "48000",
        "-ac",
        "2",
        &tone.display().to_string(),
    ]);
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        &format!("testsrc=duration={seconds}:size=320x240:rate=30"),
        "-i",
        &tone.display().to_string(),
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "pcm_s16le",
        "-shortest",
        &dir.join(name).display().to_string(),
    ]);
    let _ = std::fs::remove_file(&tone);
}

/// Video-only rawvideo in MOV: every mid-file byte is pixel data, so a
/// single flipped byte keeps the file decodable while changing the
/// picture deterministically.
fn gen_raw_clip(dir: &Path, name: &str, seconds: f64) {
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        &format!("testsrc=duration={seconds}:size=64x48:rate=30"),
        "-c:v",
        "rawvideo",
        "-pix_fmt",
        "rgb24",
        &dir.join(name).display().to_string(),
    ]);
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

fn cache_dir(folder: &Path) -> PathBuf {
    folder.join("logs").join("framesmith-footage-cache")
}

fn cache_bytes(folder: &Path) -> u64 {
    let dir = cache_dir(folder);
    std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| e.metadata().ok())
                .filter(|m| m.is_file())
                .map(|m| m.len())
                .sum()
        })
        .unwrap_or(0)
}

/// `total=(\d+)ms` from the timing summary line.
fn total_ms(output: &str) -> u64 {
    let tag = "total=";
    let at = output
        .find(tag)
        .expect("timing line reports total: {output}");
    let rest = &output[at + tag.len()..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().expect("total_ms parses: {output}")
}

/// A second render of an unchanged footage project is faster than the
/// first and byte-identical to it, with misses reported first and hits
/// on the re-render. The second clip loops a trimmed window at double
/// rate, so the cache path covers composed trim/rate/loop windows too.
#[test]
fn second_render_is_faster_and_byte_identical_with_hits_reported() {
    let root = tmp("rerender");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_clip(&folder, "clip.mp4", 4.0);
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=240 duration=6 fps=30 background=\"#000000\" version=1 {\n    video path=\"clip.mp4\" start=0 duration=4\n    video path=\"clip.mp4\" start=4 duration=2 trim-start=1 trim-end=3 rate=2 loop=true\n}\n",
    );
    let (code, first) = run(&["render", "proj", "-o", "first.mp4"], &root);
    assert_eq!(code, 0, "{first}");
    assert!(
        first.contains("cache: 0 hit(s), 2 miss(es)"),
        "first render reports misses: {first}"
    );
    let (code, second) = run(&["render", "proj", "-o", "second.mp4"], &root);
    assert_eq!(code, 0, "{second}");
    assert!(
        second.contains("cache: 2 hit(s), 0 miss(es)"),
        "second render reports hits: {second}"
    );
    let a_len = std::fs::metadata(root.join("first.mp4")).unwrap().len();
    assert!(a_len > 0);
    assert_identical_rerender(&root.join("first.mp4"), &root.join("second.mp4"), "re-render from cache stays byte-identical");
    let (t1, t2) = (total_ms(&first), total_ms(&second));
    assert!(
        t2 < t1,
        "cached re-render is faster (first {t1}ms, second {t2}ms):\n{first}\n{second}"
    );
    println!(
        "cache-report: first {t1}ms, second {t2}ms, {a_len} bytes identical"
    );
}

/// The `--json` render document carries the same hit/miss report
/// machine-readably on both renders.
#[test]
fn json_summary_reports_hits_and_misses() {
    let root = tmp("json");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_clip(&folder, "clip.mp4", 4.0);
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=240 duration=4 fps=30 background=\"#000000\" version=1 {\n    video path=\"clip.mp4\" start=0 duration=4\n}\n",
    );
    let (code, doc) = run_stdout(&["render", "proj", "-o", "j1.mp4", "--json"], &root);
    assert_eq!(code, 0, "{doc}");
    let doc = doc.trim();
    assert_eq!(doc.lines().count(), 1, "stdout stays one JSON document");
    assert!(doc.contains("\"cache\":{\"hits\":0,\"misses\":1}"), "{doc}");
    let (code, doc) = run_stdout(&["render", "proj", "-o", "j2.mp4", "--json"], &root);
    assert_eq!(code, 0, "{doc}");
    let doc = doc.trim();
    assert!(doc.contains("\"cache\":{\"hits\":1,\"misses\":0}"), "{doc}");
    assert_same_picture(&root.join("j1.mp4"), &root.join("j2.mp4"), "json re-render stays byte-identical");
}

/// Flipping one media byte forces a fresh decode: the output changes
/// (no stale picture) and the render reports a miss, not a hit.
#[test]
fn flipped_media_byte_renders_fresh_without_stale_frames() {
    let root = tmp("fresh");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_raw_clip(&folder, "clip.mov", 2.0);
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=2 fps=30 background=\"#000000\" version=1 {\n    video path=\"clip.mov\" start=0 duration=2\n}\n",
    );
    let (code, out) = run(&["render", "proj", "-o", "before.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let before = std::fs::read(root.join("before.mp4")).unwrap();
    let path = folder.join("clip.mov");
    let mut bytes = std::fs::read(&path).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0xFF;
    std::fs::write(&path, &bytes).unwrap();
    let (code, out) = run(&["render", "proj", "-o", "after.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("cache: 0 hit(s), 1 miss(es)"),
        "edited bytes miss, never hit stale: {out}"
    );
    let after = std::fs::read(root.join("after.mp4")).unwrap();
    assert_ne!(before, after, "one flipped media byte changes the output");
}

/// A missing cache directory and a corrupt entry file both degrade to a
/// correct full render that reports misses instead of hits (REC-004).
#[test]
fn missing_and_corrupt_cache_render_correctly_with_misses() {
    let root = tmp("bypass");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_clip(&folder, "clip.mp4", 4.0);
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=240 duration=4 fps=30 background=\"#000000\" version=1 {\n    video path=\"clip.mp4\" start=0 duration=4\n}\n",
    );
    let (code, out) = run(&["render", "proj", "-o", "ref.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let expected = std::fs::read(root.join("ref.mp4")).unwrap();
    assert!(cache_bytes(&folder) > 0, "first render populates the cache");

    // Missing cache: full correct render with misses.
    std::fs::remove_dir_all(cache_dir(&folder)).unwrap();
    let (code, out) = run(&["render", "proj", "-o", "nocache.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("cache: 0 hit(s), 1 miss(es)"),
        "missing cache reports misses: {out}"
    );
    // The reference bytes stay for the non-Windows byte check;
    // Windows compares decoded frames via the helper.
    let ref_path = root.join("ref.mp4");
    #[cfg(not(windows))]
    {
        assert_eq!(
            std::fs::read(root.join("nocache.mp4")).unwrap(),
            expected,
            "missing cache still renders the same bytes"
        );
    }
    #[cfg(windows)]
    assert_identical_rerender(&ref_path, &root.join("nocache.mp4"), "missing cache still renders the same bytes");

    // Corrupt cache: every entry unreadable, same correct outcome.
    for entry in std::fs::read_dir(cache_dir(&folder)).unwrap().flatten() {
        std::fs::write(entry.path(), b"garbage-not-a-cache-entry").unwrap();
    }
    // Plus one truncated entry for the partial-write case.
    if let Some(entry) = std::fs::read_dir(cache_dir(&folder))
        .unwrap()
        .flatten()
        .next()
    {
        let half = std::fs::read(entry.path()).unwrap();
        std::fs::write(entry.path(), &half[..half.len() / 2]).unwrap();
    }
    let (code, out) = run(&["render", "proj", "-o", "corrupt.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("cache: 0 hit(s), 1 miss(es)"),
        "corrupt cache reports misses: {out}"
    );
    #[cfg(not(windows))]
    assert_eq!(
        std::fs::read(root.join("corrupt.mp4")).unwrap(),
        expected,
        "corrupt cache still renders the same bytes"
    );
    #[cfg(windows)]
    assert_identical_rerender(&ref_path, &root.join("corrupt.mp4"), "corrupt cache still renders the same bytes");
}

/// Footage output does not depend on worker count, with or without a
/// warm cache.
#[test]
fn footage_output_is_worker_count_independent() {
    let root = tmp("workers");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    gen_clip(&folder, "clip.mp4", 4.0);
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=240 duration=4 fps=30 background=\"#000000\" version=1 {\n    video path=\"clip.mp4\" start=0 duration=4\n}\n",
    );
    let (code, out) = run_env(
        &["render", "proj", "-o", "w1.mp4"],
        &root,
        &[("FRAMESMITH_WORKERS", "1")],
    );
    assert_eq!(code, 0, "{out}");
    let (code, out) = run_env(
        &["render", "proj", "-o", "w8.mp4"],
        &root,
        &[("FRAMESMITH_WORKERS", "8")],
    );
    assert_eq!(code, 0, "{out}");
    #[cfg(not(windows))]
    assert_eq!(
        std::fs::read(root.join("w1.mp4")).unwrap(),
        std::fs::read(root.join("w8.mp4")).unwrap(),
        "1 worker and 8 workers render identical footage bytes"
    );
    #[cfg(windows)]
    assert_identical_rerender(&root.join("w1.mp4"), &root.join("w8.mp4"), "1 worker and 8 workers render identical footage bytes");
}

/// A tiny byte bound keeps the cache directory inside the bound while
/// the render stays byte-identical to the unbounded one: eviction only
/// ever costs speed, never correctness.
#[test]
fn eviction_keeps_output_correct_inside_the_bound() {
    let root = tmp("evict");
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    // Two distinct sources so two entries compete for the bound.
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc=duration=2:size=64x48:rate=30",
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        &folder.join("a.mp4").display().to_string(),
    ]);
    ffmpeg(&[
        "-f",
        "lavfi",
        "-i",
        "smptebars=duration=2:size=64x48:rate=30",
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        &folder.join("b.mp4").display().to_string(),
    ]);
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=64 height=48 duration=4 fps=30 background=\"#000000\" version=1 {\n    video path=\"a.mp4\" start=0 duration=2\n    video path=\"b.mp4\" start=2 duration=2\n}\n",
    );
    // One 64x48 2s entry is ~553 kB: the bound fits one but never two.
    let bound: u64 = 600_000;
    let (code, out) = run_env(
        &["render", "proj", "-o", "bounded.mp4"],
        &root,
        &[("FRAMESMITH_CACHE_MAX_BYTES", "600000")],
    );
    assert_eq!(code, 0, "{out}");
    let bounded = std::fs::read(root.join("bounded.mp4")).unwrap();
    assert!(
        cache_bytes(&folder) <= bound,
        "cache stays inside the bound ({} bytes): {out}",
        cache_bytes(&folder)
    );
    // A second bounded render is still correct (hits, misses, or a mix —
    // eviction only costs speed).
    let (code, out) = run_env(
        &["render", "proj", "-o", "bounded2.mp4"],
        &root,
        &[("FRAMESMITH_CACHE_MAX_BYTES", "600000")],
    );
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        std::fs::read(root.join("bounded2.mp4")).unwrap(),
        bounded,
        "bounded re-render stays byte-identical"
    );
    assert!(
        cache_bytes(&folder) <= bound,
        "cache stays inside the bound after re-render"
    );
    // The unbounded reference agrees byte for byte.
    std::fs::remove_dir_all(cache_dir(&folder)).unwrap();
    let (code, out) = run(&["render", "proj", "-o", "ref.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        std::fs::read(root.join("ref.mp4")).unwrap(),
        bounded,
        "eviction never changes the rendered bytes"
    );
}
