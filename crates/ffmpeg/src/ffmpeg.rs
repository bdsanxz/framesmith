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
            finding.reason = format!("could not run `{}` to read its version", ffmpeg.display());
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
        finding.reason =
            format!("ffmpeg major version {major} is older than the minimum {MIN_FFMPEG_MAJOR}");
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
        finding.reason = format!(
            "ffmpeg is missing required encoder(s): {}",
            missing.join(", ")
        );
        return finding;
    }
    finding.suitable = true;
    finding.reason = "version and encoder support confirmed".to_string();
    finding
}

/// Explicit self-download source (T009 / CAP-016). `sha256` identifies
/// the `ffmpeg` binary *inside* the downloaded archive — the file that
/// gets executed. The archive itself is only a transport wrapper.
/// Explicit sources come only from the operator/test overrides
/// (`FRAMESMITH_FFMPEG_URL`, `FRAMESMITH_FFMPEG_SHA256`,
/// `FRAMESMITH_FFMPEG_VERSION`, `FRAMESMITH_FFMPEG_ENTRY`); ordinary
/// runs resolve the latest release programmatically instead.
#[derive(Debug, Clone)]
pub struct PinSource {
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub entry: String,
}

/// Automatic self-download source: the project's own FFmpeg binary
/// registry (`bdsanxz/ffmpeg-registry` releases), resolved
/// programmatically at download time for the running OS/arch pair.
/// The registry mirrors the automated vcpkg-matrix release mechanism
/// (version check, per-OS build, `ffmpeg-{os}-{arch}.zip` assets).
/// `https://github.com/bdsanxz/ffmpeg-registry/releases`
pub const FFMPEGBIN_RELEASES_PAGE: &str = "https://github.com/bdsanxz/ffmpeg-registry/releases";
/// Release metadata endpoint queried for the newest tag and assets.
pub const FFMPEGBIN_API_LATEST: &str =
    "https://api.github.com/repos/bdsanxz/ffmpeg-registry/releases/latest";

/// FFmpeg registry baseline version published when this build was made.
/// The bootstrap resolves the latest registry release at download
/// time; this pin names the release the version command reports
/// (Sprint 27 T002 / CAP-002) and matches the `9.0.2` download URLs in
/// the release workflow and the pin named in the third-party notices.
/// Bump all three together.
pub const FFMPEG_REGISTRY_PIN: &str = "9.0.2";

/// Every shipped registry pair: `(os, arch, asset, archive entry,
/// installed binary)`. OS/arch use [`std::env::consts`] spellings
/// (`macos` for darwin, `aarch64`/`x86_64`/`x86`). The registry ships
/// exactly these seven zips; bootstrap never downloads for any other
/// pair.
pub const SUPPORTED_PLATFORMS: &[(&str, &str, &str, &str, &str)] = &[
    ("macos", "aarch64", "ffmpeg-osx-arm64.zip", "ffmpeg", "ffmpeg"),
    ("macos", "x86_64", "ffmpeg-osx-x64.zip", "ffmpeg", "ffmpeg"),
    ("linux", "x86_64", "ffmpeg-linux-x64.zip", "ffmpeg", "ffmpeg"),
    ("linux", "aarch64", "ffmpeg-linux-arm64.zip", "ffmpeg", "ffmpeg"),
    ("linux", "x86", "ffmpeg-linux-x86.zip", "ffmpeg", "ffmpeg"),
    (
        "windows",
        "x86_64",
        "ffmpeg-windows-x64.zip",
        "ffmpeg.exe",
        "ffmpeg.exe",
    ),
    (
        "windows",
        "aarch64",
        "ffmpeg-windows-arm64.zip",
        "ffmpeg.exe",
        "ffmpeg.exe",
    ),
];

/// The supported-set wording every refusal names, so an unknown
/// OS/arch pair lists what this build can download.
pub fn supported_platforms_line() -> String {
    let names: Vec<String> = SUPPORTED_PLATFORMS
        .iter()
        .map(|(os, arch, asset, entry, exe)| format!("{os}/{arch}: {asset} (entry {entry}, binary {exe})"))
        .collect();
    format!("supported platforms: {}", names.join("; "))
}

/// Asset file, archive entry, and installed binary name for one
/// OS/arch pair (Sprint 25, T001): the shipped registry contract.
/// Unknown pairs return `None`, so bootstrap refuses with the
/// supported set named instead of guessing a download.
pub fn asset_for_os_arch(os: &str, arch: &str) -> Option<(&'static str, &'static str, &'static str)> {
    SUPPORTED_PLATFORMS
        .iter()
        .find(|(o, a, _, _, _)| *o == os && *a == arch)
        .map(|(_, _, asset, entry, exe)| (*asset, *entry, *exe))
}

/// Asset file, archive entry, and installed binary name for this host,
/// decided by the running OS/arch. Platforms without a published build
/// return `None`: the bootstrap then refuses with the manual-provision
/// fix instead of guessing a download.
pub fn ffmpegbin_asset() -> Option<(&'static str, &'static str, &'static str)> {
    asset_for_os_arch(std::env::consts::OS, std::env::consts::ARCH)
}

/// Installed binary file name for this host (`ffmpeg.exe` on Windows,
/// `ffmpeg` elsewhere).
pub fn exe_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "ffmpeg.exe"
    }
    #[cfg(not(target_os = "windows"))]
    {
        "ffmpeg"
    }
}

/// One release asset from the FFmpegBin release metadata: download URL
/// plus the publisher-reported SHA256 of the archive file itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub name: String,
    pub url: String,
    /// Hex SHA256 of the downloadable archive (from the `digest` field,
    /// `sha256:` prefix stripped). Empty when the metadata carries none.
    pub archive_sha256: String,
}

/// Read a JSON `"key": "value"` string field from a flat object slice.
/// Handles `\"` and `\\` escapes; returns `None` when absent.
fn json_field(obj: &str, key: &str) -> Option<String> {
    let quoted = format!("\"{key}\"");
    let mut rest = obj;
    loop {
        let at = rest.find(quoted.as_str())?;
        let mut after = rest[at + quoted.len()..].trim_start();
        if !after.starts_with(':') {
            rest = &rest[at + quoted.len()..];
            continue;
        }
        after = after[1..].trim_start();
        if !after.starts_with('"') {
            return None;
        }
        let mut out = String::new();
        let mut esc = false;
        for c in after[1..].chars() {
            if esc {
                out.push(c);
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                return Some(out);
            } else {
                out.push(c);
            }
        }
        return None;
    }
}

/// Split a JSON array body into its top-level object slices by brace
/// depth, skipping over string literals.
fn top_level_objects(body: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i] != b'{' {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let start = i;
        let mut depth = 0u32;
        let mut instr = false;
        let mut esc = false;
        while i < bytes.len() {
            let b = bytes[i];
            if instr {
                if esc {
                    esc = false;
                } else if b == b'\\' {
                    esc = true;
                } else if b == b'"' {
                    instr = false;
                }
            } else if b == b'"' {
                instr = true;
            } else if b == b'{' {
                depth += 1;
            } else if b == b'}' {
                depth -= 1;
                if depth == 0 {
                    i += 1;
                    break;
                }
            }
            i += 1;
        }
        out.push(&body[start..i.min(body.len())]);
    }
    out
}

/// Parse release metadata into its tag plus usable assets. Assets
/// without a download URL or without a valid `sha256:` digest are
/// skipped: verification material is mandatory, never optional.
pub fn parse_release(body: &str) -> Result<(String, Vec<ReleaseAsset>), String> {
    let tag = json_field(body, "tag_name")
        .filter(|t| !t.is_empty())
        .ok_or_else(|| "release metadata has no usable tag_name".to_string())?;
    let assets_at = body
        .find("\"assets\"")
        .ok_or_else(|| "release metadata has no assets list".to_string())?;
    let open = body[assets_at..]
        .find('[')
        .ok_or_else(|| "release metadata has no assets list".to_string())?;
    let mut depth = 0u32;
    let mut instr = false;
    let mut esc = false;
    let mut end = None;
    for (n, b) in body[assets_at + open..].bytes().enumerate() {
        if instr {
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                instr = false;
            }
        } else if b == b'"' {
            instr = true;
        } else if b == b'[' {
            depth += 1;
        } else if b == b']' {
            depth -= 1;
            if depth == 0 {
                end = Some(assets_at + open + n);
                break;
            }
        }
    }
    let end = end.ok_or_else(|| "release metadata has a truncated assets list".to_string())?;
    let mut assets = Vec::new();
    for obj in top_level_objects(&body[assets_at + open..end]) {
        let (Some(name), Some(url), Some(digest)) = (
            json_field(obj, "name"),
            json_field(obj, "browser_download_url"),
            json_field(obj, "digest"),
        ) else {
            continue;
        };
        if name.is_empty() || url.is_empty() {
            continue;
        }
        let Some(hex) = digest.strip_prefix("sha256:") else {
            continue;
        };
        if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        assets.push(ReleaseAsset {
            name,
            url,
            archive_sha256: hex.to_string(),
        });
    }
    if assets.is_empty() {
        return Err("release metadata carries no verifiable asset".to_string());
    }
    Ok((tag, assets))
}

/// Pick the asset matching this host from parsed release metadata.
pub fn select_asset<'a>(assets: &'a [ReleaseAsset], wanted: &str) -> Option<&'a ReleaseAsset> {
    assets.iter().find(|a| a.name == wanted)
}

/// Fetch release metadata over HTTPS with `curl` (bounded time, no
/// hangs). Used only for automatic latest-resolution; tests and the
/// explicit-override path never touch the network.
pub fn fetch_release(api_url: &str, timeout_secs: u64) -> Result<String, String> {
    let curl = find_helper("curl")
        .ok_or_else(|| "no `curl` found on PATH to resolve the latest release".to_string())?;
    let out = std::process::Command::new(&curl)
        .args([
            "-fsSL",
            "--connect-timeout",
            "20",
            "--max-time",
            &timeout_secs.to_string(),
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "User-Agent: framesmith",
        ])
        .arg(api_url)
        .output()
        .map_err(|e| format!("could not launch `{}`: {e}", curl.display()))?;
    if !out.status.success() {
        let mut detail = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if detail.is_empty() {
            detail = format!("curl exited with status {}", out.status);
        }
        if detail.len() > 300 {
            detail.truncate(300);
        }
        return Err(format!("release lookup failed: {detail}"));
    }
    String::from_utf8(out.stdout)
        .map_err(|_| "release lookup returned non-UTF-8 metadata".to_string())
}

/// Resolve the effective explicit download source. All four overrides
/// (`FRAMESMITH_FFMPEG_URL`, `FRAMESMITH_FFMPEG_SHA256`,
/// `FRAMESMITH_FFMPEG_VERSION`, `FRAMESMITH_FFMPEG_ENTRY`) must be set —
/// this is how tests serve a local fixture and how an operator pins a
/// build without rebuilding. Any missing field means there is no
/// explicit source, so this returns `None` and the bootstrap resolves
/// the latest release programmatically instead.
pub fn source_from_env() -> Option<PinSource> {
    let get = |key: &str| std::env::var(key).ok().filter(|v| !v.is_empty());
    Some(PinSource {
        url: get("FRAMESMITH_FFMPEG_URL")?,
        sha256: get("FRAMESMITH_FFMPEG_SHA256")?,
        version: get("FRAMESMITH_FFMPEG_VERSION")?,
        entry: get("FRAMESMITH_FFMPEG_ENTRY")?,
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

/// Installed location of a verified build:
/// `<cache>/<version>/bin/ffmpeg` (`ffmpeg.exe` on Windows).
pub fn cached_binary(cache_dir: &Path, version: &str) -> PathBuf {
    cache_dir.join(version).join("bin").join(exe_name())
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
        let (v, m) =
            parse_version("ffmpeg version 9.0.2 Copyright (c) 2000-2026\nbuilt with").unwrap();
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
    fn host_asset_mapping_is_complete() {
        match ffmpegbin_asset() {
            Some((asset, entry, exe)) => {
                assert!(asset.starts_with("ffmpeg-") && asset.ends_with(".zip"));
                assert!(!entry.is_empty());
                assert_eq!(exe, exe_name());
                assert!(FFMPEGBIN_API_LATEST.starts_with("https://"));
                assert!(FFMPEGBIN_RELEASES_PAGE.starts_with("https://"));
            }
            None => {
                // Platforms without a published build: documented gap, not a failure.
            }
        }
    }

    const RELEASE_FIXTURE: &str = r#"{"tag_name": "9.0.2", "assets": [
        {"name": "ffmpeg-osx-arm64.zip", "browser_download_url": "https://example.invalid/a.zip",
         "digest": "sha256:939127a5eb284f0eab311a42f1d1f3fd1b088e2c0088f573c7cb2b4e59b0e81b"},
        {"name": "ffmpeg-osx-x64.zip", "browser_download_url": "",
         "digest": "sha256:7facc503c7d67e1e58d11468c9c183fedcb7974490b589c079dfda2a9e57fb10"},
        {"name": "ffmpeg-linux-x64.zip", "browser_download_url": "https://example.invalid/l.zip",
         "digest": "md5:deadbeef"},
        {"name": "notes.txt", "browser_download_url": "https://example.invalid/n.txt"}
    ]}"#;

    #[test]
    fn release_metadata_parses_and_selects_verifiable_assets() {
        let (tag, assets) = parse_release(RELEASE_FIXTURE).unwrap();
        assert_eq!(tag, "9.0.2");
        // Only the fully verifiable asset survives: empty URL, non-sha256
        // digest, and missing digest are all skipped, never trusted.
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].name, "ffmpeg-osx-arm64.zip");
        assert_eq!(assets[0].archive_sha256.len(), 64);
        let picked = select_asset(&assets, "ffmpeg-osx-arm64.zip").unwrap();
        assert_eq!(picked.url, "https://example.invalid/a.zip");
        assert!(select_asset(&assets, "ffmpeg-windows-x64.zip").is_none());
        // Garbage metadata fails closed instead of resolving anything.
        assert!(parse_release("{}").is_err());
        assert!(parse_release(r#"{"tag_name": "", "assets": []}"#).is_err());
        assert!(parse_release(r#"{"tag_name": "1.0", "assets": []}"#).is_err());
    }

    #[test]
    fn explicit_source_needs_all_four_overrides() {
        std::env::remove_var("FRAMESMITH_FFMPEG_URL");
        std::env::remove_var("FRAMESMITH_FFMPEG_SHA256");
        std::env::remove_var("FRAMESMITH_FFMPEG_VERSION");
        std::env::remove_var("FRAMESMITH_FFMPEG_ENTRY");
        assert!(source_from_env().is_none());
        std::env::set_var("FRAMESMITH_FFMPEG_URL", "file:///tmp/x.zip");
        assert!(
            source_from_env().is_none(),
            "a lone URL is not a verifiable source"
        );
        std::env::set_var("FRAMESMITH_FFMPEG_SHA256", &"1".repeat(64));
        std::env::set_var("FRAMESMITH_FFMPEG_VERSION", "9.0-t");
        std::env::set_var("FRAMESMITH_FFMPEG_ENTRY", "ffmpeg");
        let explicit = source_from_env().expect("four overrides form a source");
        assert_eq!(explicit.version, "9.0-t");
        std::env::remove_var("FRAMESMITH_FFMPEG_URL");
        std::env::remove_var("FRAMESMITH_FFMPEG_SHA256");
        std::env::remove_var("FRAMESMITH_FFMPEG_VERSION");
        std::env::remove_var("FRAMESMITH_FFMPEG_ENTRY");
    }

    #[test]
    fn every_shipped_pair_resolves_its_registry_names() {
        // Sprint 25 T001: the registry contract, exact asset / entry /
        // binary names for all seven pairs.
        let table = [
            (
                "macos",
                "aarch64",
                ("ffmpeg-osx-arm64.zip", "ffmpeg", "ffmpeg"),
            ),
            ("macos", "x86_64", ("ffmpeg-osx-x64.zip", "ffmpeg", "ffmpeg")),
            (
                "linux",
                "x86_64",
                ("ffmpeg-linux-x64.zip", "ffmpeg", "ffmpeg"),
            ),
            (
                "linux",
                "aarch64",
                ("ffmpeg-linux-arm64.zip", "ffmpeg", "ffmpeg"),
            ),
            (
                "linux",
                "x86",
                ("ffmpeg-linux-x86.zip", "ffmpeg", "ffmpeg"),
            ),
            (
                "windows",
                "x86_64",
                ("ffmpeg-windows-x64.zip", "ffmpeg.exe", "ffmpeg.exe"),
            ),
            (
                "windows",
                "aarch64",
                ("ffmpeg-windows-arm64.zip", "ffmpeg.exe", "ffmpeg.exe"),
            ),
        ];
        assert_eq!(SUPPORTED_PLATFORMS.len(), 7);
        for (os, arch, want) in table {
            assert_eq!(
                asset_for_os_arch(os, arch),
                Some(want),
                "{os}/{arch} must resolve its shipped asset"
            );
        }
        // The host delegates to the table for the running pair.
        let host = ffmpegbin_asset();
        assert_eq!(host, asset_for_os_arch(std::env::consts::OS, std::env::consts::ARCH));
        if let Some((asset, entry, exe)) = host {
            assert!(asset.ends_with(".zip"), "{asset}");
            assert_eq!(exe, exe_name());
            assert!(!entry.is_empty());
        }
        // The table agrees with the public lookup both ways: every row
        // resolves, and no row is a duplicate pair.
        for (i, (os, arch, asset, entry, exe)) in SUPPORTED_PLATFORMS.iter().enumerate() {
            assert_eq!(asset_for_os_arch(os, arch), Some((*asset, *entry, *exe)));
            for (os2, arch2, ..) in &SUPPORTED_PLATFORMS[i + 1..] {
                assert!(*os != *os2 || *arch != *arch2, "duplicate pair {os}/{arch}");
            }
        }
    }

    #[test]
    fn unknown_pairs_refuse_with_the_supported_set_named() {
        for (os, arch) in [
            ("macos", "arm64"),
            ("darwin", "aarch64"),
            ("windows", "x86"),
            ("freebsd", "x86_64"),
            ("linux", "riscv64"),
            ("linux", "armv7"),
            ("", ""),
        ] {
            assert_eq!(asset_for_os_arch(os, arch), None, "{os}/{arch}");
        }
        let line = supported_platforms_line();
        for asset in [
            "ffmpeg-osx-arm64.zip",
            "ffmpeg-osx-x64.zip",
            "ffmpeg-linux-x64.zip",
            "ffmpeg-linux-arm64.zip",
            "ffmpeg-linux-x86.zip",
            "ffmpeg-windows-x64.zip",
            "ffmpeg-windows-arm64.zip",
        ] {
            assert!(line.contains(asset), "{line}");
        }
        assert!(line.contains("supported platforms"), "{line}");
    }

    #[test]
    fn cached_binary_layout() {
        let dir = std::path::Path::new("/cache");
        assert_eq!(
            cached_binary(dir, "9.0"),
            dir.join("9.0").join("bin").join(exe_name())
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
