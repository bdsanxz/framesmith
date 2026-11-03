//! FFmpeg detection (T008 / CAP-015).
//!
//! Before any decode or encode, the system locates `ffmpeg` and `ffprobe`
//! on `PATH`, verifies the version and the required encoders, and records
//! the finding for the render decision. Detection only reads host state:
//! it never modifies the author folder.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Minimum accepted FFmpeg major version. Recorded in code as the
/// downstream fact; a binary older than this is unsuitable.
pub const MIN_FFMPEG_MAJOR: u64 = 6;

/// Required encoders for the Sprint 1 MP4 (H.264 video + AAC audio).
pub const REQUIRED_VIDEO_ENCODER: &str = "libx264";
pub const REQUIRED_AUDIO_ENCODER: &str = "aac";

/// The recorded host-state finding. `suitable == true` means render may
/// proceed; anything else moves to bootstrap/refusal handling (T009/T010).
#[derive(Debug, Clone)]
pub struct FfmpegFinding {
    pub ffmpeg_path: Option<PathBuf>,
    pub ffprobe_path: Option<PathBuf>,
    pub version: Option<String>,
    pub major: Option<u64>,
    pub has_video_encoder: bool,
    pub has_audio_encoder: bool,
    pub suitable: bool,
    /// Human-readable reason: suitability confirmation or the blocking cause.
    pub reason: String,
}

impl std::fmt::Display for FfmpegFinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.suitable {
            write!(
                f,
                "ffmpeg: suitable ({} at {}, encoders {}/{} present)",
                self.version.as_deref().unwrap_or("unknown version"),
                self.ffmpeg_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "?".into()),
                REQUIRED_VIDEO_ENCODER,
                REQUIRED_AUDIO_ENCODER,
            )
        } else {
            write!(f, "ffmpeg: unsuitable ({})", self.reason)
        }
    }
}

fn find_on_path_in(name: &str, path_value: Option<&OsStr>) -> Option<PathBuf> {
    let path_var = match path_value {
        Some(v) => v.to_os_string(),
        None => std::env::var_os("PATH")?,
    };
    for dir in std::env::split_paths(&path_var) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let exe = dir.join(format!("{name}.exe"));
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    None
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    find_on_path_in(name, None)
}

/// Locate a helper tool (`curl`, `unzip`, ...) on `PATH`.
pub fn find_helper(name: &str) -> Option<PathBuf> {
    find_on_path(name)
}

fn run_capture(path: &std::path::Path, args: &[&str]) -> Option<String> {
    let out = Command::new(path).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    Some(text)
}

/// Parse `ffmpeg version 9.0.2 ...` (or `ffprobe version ...`) output.
/// Returns (full version token, major).
pub fn parse_version(output: &str) -> Option<(String, u64)> {
    for line in output.lines() {
        let line = line.trim();
        for prefix in ["ffmpeg version ", "ffprobe version "] {
            if let Some(rest) = line.strip_prefix(prefix) {
                let token: String = rest.split_whitespace().next()?.to_string();
                let major = token.split('.').next()?.parse::<u64>().ok()?;
                return Some((token, major));
            }
        }
    }
    None
}

/// Parse `ffmpeg -encoders` output for the required encoder names.
pub fn parse_encoders(output: &str, wanted: &str) -> bool {
    output.lines().any(|line| {
        let mut parts = line.split_whitespace();
        // Encoder rows look like ` V..... libx264 ...` — the second
        // whitespace-separated token is the encoder name.
        parts.next();
        parts.next() == Some(wanted)
    })
}

/// Locate `ffmpeg`/`ffprobe`, verify version and encoders, record finding.
pub fn detect() -> FfmpegFinding {
    detect_with_path(std::env::var_os("PATH"))
}

/// Same as [`detect`] but against an explicit `PATH` value, so tests can
/// simulate a machine without FFmpeg without touching process state.
pub fn detect_with_path(path_value: Option<OsString>) -> FfmpegFinding {
    let path_ref = path_value.as_deref();
    let ffmpeg_path = find_on_path_in("ffmpeg", path_ref);
    let ffprobe_path = find_on_path_in("ffprobe", path_ref);

    let mut finding = FfmpegFinding {
        ffmpeg_path: ffmpeg_path.clone(),
        ffprobe_path: ffprobe_path.clone(),
        version: None,
        major: None,
        has_video_encoder: false,
        has_audio_encoder: false,
        suitable: false,
        reason: String::new(),
    };

    let ffmpeg = match ffmpeg_path {
        Some(p) => p,
        None => {
            finding.reason = "no `ffmpeg` binary found on PATH".to_string();
            return finding;
        }
    };
    if ffprobe_path.is_none() {
        finding.reason = "found `ffmpeg` but no `ffprobe` on PATH".to_string();
        return finding;
    }
    let version_out = match run_capture(&ffmpeg, &["-version"]) {
        Some(o) => o,
        None => {
            finding.reason = format!(
                "could not run `{}` to read its version",
                ffmpeg.display()
            );
            return finding;
        }
    };
    let (version, major) = match parse_version(&version_out) {
        Some(v) => v,
        None => {
            finding.reason = "could not parse the ffmpeg version string".to_string();
            return finding;
        }
    };
    finding.version = Some(version);
    finding.major = Some(major);
    if major < MIN_FFMPEG_MAJOR {
        finding.reason = format!(
            "ffmpeg major version {major} is older than the minimum {MIN_FFMPEG_MAJOR}"
        );
        return finding;
    }
    let encoders_out = run_capture(&ffmpeg, &["-hide_banner", "-encoders"]).unwrap_or_default();
    finding.has_video_encoder = parse_encoders(&encoders_out, REQUIRED_VIDEO_ENCODER);
    finding.has_audio_encoder = parse_encoders(&encoders_out, REQUIRED_AUDIO_ENCODER);
    if !finding.has_video_encoder || !finding.has_audio_encoder {
        let mut missing = Vec::new();
        if !finding.has_video_encoder {
            missing.push(REQUIRED_VIDEO_ENCODER);
        }
        if !finding.has_audio_encoder {
            missing.push(REQUIRED_AUDIO_ENCODER);
        }
        finding.reason = format!("ffmpeg is missing required encoder(s): {}", missing.join(", "));
        return finding;
    }
    finding.suitable = true;
    finding.reason = "version and encoder support confirmed".to_string();
    finding
}

/// Pinned self-download source (T009 / CAP-016). `sha256` identifies the
/// `ffmpeg` binary *inside* the downloaded archive — the file that gets
/// executed. The archive itself is only a transport wrapper.
#[derive(Debug, Clone)]
pub struct PinSource {
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub entry: String,
}

/// Recorded pinned build for this host (macOS static builds; the hashes
/// are the publishers' published SHA256 values for the extracted binary,
/// re-verified locally at implementation time). Platforms without a pin
/// return `None`: the bootstrap then refuses with the manual-provision
/// fix instead of guessing a download.
pub fn pinned_source() -> Option<PinSource> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        Some(PinSource {
            version: "9.0".to_string(),
            url: "https://www.osxexperts.net/ffmpeg9arm.zip".to_string(),
            sha256: "591260c945d0eef150e3bf82b0ef988bd36a9cecc18ff05d6679617159f0a95e"
                .to_string(),
            entry: "ffmpeg".to_string(),
        })
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        Some(PinSource {
            version: "8.0".to_string(),
            url: "https://www.osxexperts.net/ffmpeg80intel.zip".to_string(),
            sha256: "df3f1e3facdc1ae0ad0bd898cdfb072fbc9641bf47b11f172844525a05db8d11"
                .to_string(),
            entry: "ffmpeg".to_string(),
        })
    }
    #[cfg(not(all(target_os = "macos", any(target_arch = "aarch64", target_arch = "x86_64"))))]
    {
        None
    }
}

/// Resolve the effective download source. Explicit overrides
/// (`FRAMESMITH_FFMPEG_URL`, `FRAMESMITH_FFMPEG_SHA256`,
/// `FRAMESMITH_FFMPEG_VERSION`, `FRAMESMITH_FFMPEG_ENTRY`) win field by
/// field — this is how tests serve a local fixture and how an operator
/// could re-pin without rebuilding. Any missing field means there is no
/// verifiable source, so this returns `None`.
pub fn source_from_env() -> Option<PinSource> {
    let pinned = pinned_source();
    let pick = |key: &str, fallback: Option<String>| {
        std::env::var(key)
            .ok()
            .filter(|v| !v.is_empty())
            .or(fallback)
    };
    Some(PinSource {
        url: pick("FRAMESMITH_FFMPEG_URL", pinned.as_ref().map(|p| p.url.clone()))?,
        sha256: pick(
            "FRAMESMITH_FFMPEG_SHA256",
            pinned.as_ref().map(|p| p.sha256.clone()),
        )?,
        version: pick(
            "FRAMESMITH_FFMPEG_VERSION",
            pinned.as_ref().map(|p| p.version.clone()),
        )?,
        entry: pick(
            "FRAMESMITH_FFMPEG_ENTRY",
            pinned.as_ref().map(|p| p.entry.clone()),
        )?,
    })
}

/// Tool cache location: `FRAMESMITH_FFMPEG_DIR` wins when set (tests and
/// managed installs), otherwise `~/.framesmith/ffmpeg`. Only this
/// location — plus short-lived temp files inside it — is ever written by
/// self-download; the composition folder is untouched.
pub fn tool_cache_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("FRAMESMITH_FFMPEG_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".framesmith").join("ffmpeg")
}

/// Installed location of a verified build: `<cache>/<version>/bin/ffmpeg`.
pub fn cached_binary(cache_dir: &Path, version: &str) -> PathBuf {
    cache_dir.join(version).join("bin").join("ffmpeg")
}

/// Explicit offline switch (tests and policy-managed hosts): when
/// `FRAMESMITH_OFFLINE` is `1` the bootstrap refuses without attempting
/// any network access.
pub fn offline_forced() -> bool {
    std::env::var("FRAMESMITH_OFFLINE")
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// Download time budget in seconds. `FRAMESMITH_DOWNLOAD_TIMEOUT_SECS`
/// overrides the default (tests use a small value); always clamped.
pub fn download_timeout_secs() -> u64 {
    std::env::var("FRAMESMITH_DOWNLOAD_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(|v| v.clamp(3, 900))
        .unwrap_or(240)
}

/// Verify one explicit binary: run `<path> -version` and
/// `<path> -encoders` and apply the same suitability bar as [`detect`].
/// Returns the version string on success.
pub fn verify_binary(path: &Path) -> Result<String, String> {
    let version_out = run_capture(path, &["-version"])
        .ok_or_else(|| format!("could not run `{}` to read its version", path.display()))?;
    let (version, major) = parse_version(&version_out)
        .ok_or_else(|| format!("could not parse the version string of `{}`", path.display()))?;
    if major < MIN_FFMPEG_MAJOR {
        return Err(format!(
            "ffmpeg major version {major} at `{}` is older than the minimum {MIN_FFMPEG_MAJOR}",
            path.display()
        ));
    }
    let encoders_out = run_capture(path, &["-hide_banner", "-encoders"]).unwrap_or_default();
    let mut missing = Vec::new();
    if !parse_encoders(&encoders_out, REQUIRED_VIDEO_ENCODER) {
        missing.push(REQUIRED_VIDEO_ENCODER);
    }
    if !parse_encoders(&encoders_out, REQUIRED_AUDIO_ENCODER) {
        missing.push(REQUIRED_AUDIO_ENCODER);
    }
    if !missing.is_empty() {
        return Err(format!(
            "ffmpeg at `{}` is missing required encoder(s): {}",
            path.display(),
            missing.join(", ")
        ));
    }
    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_lines_parse() {
        let (v, m) = parse_version("ffmpeg version 9.0.2 Copyright (c) 2000-2026\nbuilt with").unwrap();
        assert_eq!((v.as_str(), m), ("9.0.2", 9));
        let (v, m) = parse_version("ffprobe version 6.1.1\n").unwrap();
        assert_eq!((v.as_str(), m), ("6.1.1", 6));
        assert!(parse_version("garbage output").is_none());
    }

    #[test]
    fn encoder_rows_match_exact_names() {
        let out = "Encoders:\n V..... libx264   libx264 H.264\n A..... aac        AAC\n";
        assert!(parse_encoders(out, "libx264"));
        assert!(parse_encoders(out, "aac"));
        assert!(!parse_encoders(out, "libx265"));
        // A longer name must not match by prefix.
        assert!(!parse_encoders(" V..... libx264rgb\n", "libx264"));
    }

    #[test]
    fn empty_path_simulates_a_machine_without_ffmpeg() {
        let finding = detect_with_path(Some(std::ffi::OsString::from(
            "/nonexistent-framesmith-noffmpeg-dir",
        )));
        assert!(!finding.suitable);
        assert!(finding.ffmpeg_path.is_none());
        assert!(!finding.reason.is_empty());
    }

    #[test]
    fn pinned_source_is_complete_and_verifiable() {
        match pinned_source() {
            Some(p) => {
                assert!(!p.version.is_empty());
                assert!(p.url.starts_with("https://"));
                assert_eq!(p.sha256.len(), 64);
                assert!(p.sha256.chars().all(|c| c.is_ascii_hexdigit()));
                assert!(!p.entry.is_empty());
            }
            None => {
                // Platforms without a recorded pin: documented gap, not a failure.
            }
        }
    }

    #[test]
    fn cached_binary_layout() {
        let dir = std::path::Path::new("/cache");
        assert_eq!(
            cached_binary(dir, "9.0"),
            std::path::Path::new("/cache/9.0/bin/ffmpeg")
        );
    }

    #[test]
    fn detect_reads_host_state_without_touching_folders() {
        let before = std::env::var_os("PATH");
        let finding = detect();
        // Detection must complete and report a non-empty reason.
        assert!(!finding.reason.is_empty());
        assert_eq!(std::env::var_os("PATH"), before);
        if finding.ffmpeg_path.is_some() && finding.ffprobe_path.is_some() {
            assert!(finding.version.is_some());
            assert!(finding.major.is_some());
        }
    }
}
