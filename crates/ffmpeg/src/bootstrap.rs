//! FFmpeg bootstrap: pinned self-download (T009) and disabled/offline
//! refusal (T010).
//!
//! Decision order when a render needs FFmpeg but the host finding is not
//! suitable:
//!
//! 1. Self-download disabled by configuration → refuse (`E_NO_FFMPEG`),
//!    naming the expected source and the disabling configuration.
//! 2. Offline → refuse (`E_NO_FFMPEG`) with the manual-provision fix.
//! 3. No recorded pin for this platform → refuse (`E_NO_FFMPEG`) with the
//!    manual-provision fix.
//! 4. Otherwise download the pinned archive with `curl` (bounded time, no
//!    hangs), extract the entry, verify its SHA256, verify the binary
//!    (`-version` plus required encoders), install it into the tool cache,
//!    announce it, and render with it. Any failure → refuse
//!    (`E_DOWNLOAD_FAILED`) with the cause; no MP4 is written and the
//!    composition folder is unchanged.
//!
//! FFmpeg only ever runs as a separate process: the downloaded file is
//! executed, never linked or shipped inside this binary.

use crate::ffmpeg::{self, FfmpegFinding, PinSource};
use crate::sha256;
use std::path::{Path, PathBuf};

/// All inputs the bootstrap decision needs. Built from the environment
/// by [`BootstrapConfig::from_env`]; tests construct it directly so no
/// test mutates process state.
#[derive(Debug, Clone)]
pub struct BootstrapConfig {
    pub auto_download: bool,
    pub offline: bool,
    pub source: Option<PinSource>,
    pub cache_dir: PathBuf,
    pub curl_timeout_secs: u64,
}

impl BootstrapConfig {
    pub fn from_env(auto_download: bool) -> Self {
        Self {
            auto_download,
            offline: ffmpeg::offline_forced(),
            source: ffmpeg::source_from_env(),
            cache_dir: ffmpeg::tool_cache_dir(),
            curl_timeout_secs: ffmpeg::download_timeout_secs(),
        }
    }
}

/// Bootstrap outcome: either a binary to execute, or a refusal.
#[derive(Debug, Clone)]
pub enum Bootstrap {
    Ready {
        ffmpeg_path: PathBuf,
        downloaded: bool,
        announcements: Vec<String>,
    },
    Refused {
        code: &'static str,
        message: String,
    },
}

fn expected_source_line(source: Option<&PinSource>) -> String {
    match source {
        Some(s) => format!(
            "pinned FFmpeg {} build ({}; SHA256 {})",
            s.version, s.url, s.sha256
        ),
        None => "a manually installed FFmpeg".to_string(),
    }
}

fn install_line() -> String {
    format!(
        "install FFmpeg {}+ with {} and {} encoders on PATH",
        ffmpeg::MIN_FFMPEG_MAJOR,
        ffmpeg::REQUIRED_VIDEO_ENCODER,
        ffmpeg::REQUIRED_AUDIO_ENCODER
    )
}

fn disabled_message(host_reason: &str, source: Option<&PinSource>) -> String {
    format!(
        "render refused before the first frame (E_NO_FFMPEG): no suitable FFmpeg ({host_reason}).\n\
         Self-download is disabled by {} (`ffmpeg_auto_download = false`).\n\
         fix: {} (see {} for the expected build), or set `ffmpeg_auto_download = true` in {} \
         to let Framesmith download it without asking. No MP4 was written.",
        notation::model::CONFIG_FILE,
        install_line(),
        expected_source_line(source),
        notation::model::CONFIG_FILE,
    )
}

fn offline_message(host_reason: &str, source: Option<&PinSource>) -> String {
    format!(
        "render refused before the first frame (E_NO_FFMPEG): no suitable FFmpeg ({host_reason}), \
         and self-download cannot reach the network (offline or FRAMESMITH_OFFLINE=1).\n\
         fix: {} and re-run, or re-run with network access so Framesmith can download {}. \
         No MP4 was written and no partial file was left behind. \
         Validation still works: `framesmith check <folder>`.",
        install_line(),
        expected_source_line(source),
    )
}

fn no_pin_message(host_reason: &str) -> String {
    format!(
        "render refused before the first frame (E_NO_FFMPEG): no suitable FFmpeg ({host_reason}), \
         and no pinned FFmpeg build is recorded for this platform.\n\
         fix: {} and re-run. No MP4 was written. \
         Validation still works: `framesmith check <folder>`.",
        install_line(),
    )
}

fn download_failed_message(host_reason: &str, source: &PinSource, cause: &str) -> String {
    format!(
        "render refused before the first frame (E_DOWNLOAD_FAILED): no suitable FFmpeg ({host_reason}); \
         self-download of the pinned FFmpeg {} build failed: {cause}.\n\
         fix: check network access and re-run to retry (the folder is unchanged), or {} manually \
         (pinned build: {}; SHA256 {}). No MP4 was written. \
         Validation still works: `framesmith check <folder>`.",
        source.version,
        install_line(),
        source.url,
        source.sha256,
    )
}

/// Apply the bootstrap decision to a host finding.
pub fn ensure_ffmpeg(host: &FfmpegFinding, cfg: &BootstrapConfig) -> Bootstrap {
    if host.suitable {
        if let Some(path) = host.ffmpeg_path.clone() {
            return Bootstrap::Ready {
                ffmpeg_path: path,
                downloaded: false,
                announcements: Vec::new(),
            };
        }
    }
    let host_reason = if host.reason.is_empty() {
        "unsuitable host binary".to_string()
    } else {
        host.reason.clone()
    };
    if !cfg.auto_download {
        return Bootstrap::Refused {
            code: "E_NO_FFMPEG",
            message: disabled_message(&host_reason, cfg.source.as_ref()),
        };
    }
    if cfg.offline {
        return Bootstrap::Refused {
            code: "E_NO_FFMPEG",
            message: offline_message(&host_reason, cfg.source.as_ref()),
        };
    }
    let source = match &cfg.source {
        Some(s) => s.clone(),
        None => {
            return Bootstrap::Refused {
                code: "E_NO_FFMPEG",
                message: no_pin_message(&host_reason),
            };
        }
    };
    let cached = ffmpeg::cached_binary(&cfg.cache_dir, &source.version);
    if cached.is_file() {
        match ffmpeg::verify_binary(&cached) {
            Ok(version) => {
                return Bootstrap::Ready {
                    ffmpeg_path: cached.clone(),
                    downloaded: false,
                    announcements: vec![format!(
                        "reusing cached pinned FFmpeg {version} at {} (SHA256 was verified on download).",
                        cached.display()
                    )],
                };
            }
            Err(_) => {
                // Stale cache entry: drop it and fall through to a download.
                let _ = std::fs::remove_file(&cached);
            }
        }
    }
    match download_and_verify(&source, cfg) {
        Ok(path) => Bootstrap::Ready {
            ffmpeg_path: path.clone(),
            downloaded: true,
            announcements: vec![format!(
                "downloaded pinned FFmpeg {} from {} (SHA256 verified) to {}; rendering with it.",
                source.version,
                source.url,
                path.display()
            )],
        },
        Err(cause) => Bootstrap::Refused {
            code: "E_DOWNLOAD_FAILED",
            message: download_failed_message(&host_reason, &source, &cause),
        },
    }
}

fn rm_rf(path: &Path) {
    if path.is_dir() {
        let _ = std::fs::remove_dir_all(path);
    } else {
        let _ = std::fs::remove_file(path);
    }
}

/// Download the pinned archive, verify entry SHA256 and binary, install
/// into the cache. Every temp artifact is removed on failure.
fn download_and_verify(source: &PinSource, cfg: &BootstrapConfig) -> Result<PathBuf, String> {
    let curl = ffmpeg::find_helper("curl")
        .ok_or_else(|| "no `curl` found on PATH to fetch the pinned build".to_string())?;
    let unzip = ffmpeg::find_helper("unzip")
        .ok_or_else(|| "no `unzip` found on PATH to unpack the pinned build".to_string())?;
    if let Err(e) = std::fs::create_dir_all(&cfg.cache_dir) {
        return Err(format!(
            "cannot create tool cache {}: {e}",
            cfg.cache_dir.display()
        ));
    }
    let tag = format!("{}-{}", std::process::id(), source.version.replace('/', "_"));
    let tmp_zip = cfg.cache_dir.join(format!(".download-{tag}.zip"));
    let stage = cfg.cache_dir.join(format!(".stage-{tag}"));
    rm_rf(&tmp_zip);
    rm_rf(&stage);
    let cleanup = |tmp_zip: &Path, stage: &Path| {
        rm_rf(tmp_zip);
        rm_rf(stage);
    };

    let timeout = cfg.curl_timeout_secs.to_string();
    let curl_out = std::process::Command::new(&curl)
        .args([
            "-fsSL",
            "--connect-timeout",
            "20",
            "--max-time",
            &timeout,
            "-o",
        ])
        .arg(&tmp_zip)
        .arg(&source.url)
        .output()
        .map_err(|e| format!("could not launch `{}`: {e}", curl.display()))?;
    if !curl_out.status.success() {
        cleanup(&tmp_zip, &stage);
        let mut detail = String::from_utf8_lossy(&curl_out.stderr).trim().to_string();
        if detail.is_empty() {
            detail = format!("curl exited with status {}", curl_out.status);
        }
        // Truncate curl chatter; keep the cause.
        if detail.len() > 300 {
            detail.truncate(300);
        }
        return Err(format!("download failed: {detail}"));
    }

    if let Err(e) = std::fs::create_dir_all(&stage) {
        cleanup(&tmp_zip, &stage);
        return Err(format!("cannot create staging dir: {e}"));
    }
    let unzip_out = std::process::Command::new(&unzip)
        .args(["-q", "-o"])
        .arg(&tmp_zip)
        .arg(&source.entry)
        .args(["-d"])
        .arg(&stage)
        .output()
        .map_err(|e| format!("could not launch `{}`: {e}", unzip.display()))?;
    if !unzip_out.status.success() {
        cleanup(&tmp_zip, &stage);
        return Err(format!(
            "could not unpack entry {:?} from the downloaded archive",
            source.entry
        ));
    }
    let staged = stage.join(&source.entry);
    if !staged.is_file() {
        cleanup(&tmp_zip, &stage);
        return Err(format!(
            "downloaded archive did not contain entry {:?}",
            source.entry
        ));
    }
    let bytes =
        std::fs::read(&staged).map_err(|e| format!("cannot read downloaded entry: {e}"))?;
    let found = sha256::sha256_hex(&bytes);
    if found.to_lowercase() != source.sha256.to_lowercase() {
        cleanup(&tmp_zip, &stage);
        return Err(format!(
            "checksum mismatch for the pinned build: expected {} but found {found}",
            source.sha256
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755));
    }
    // Strip quarantine metadata left by some fetch paths; best effort only.
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("/usr/bin/xattr")
            .args(["-cr"])
            .arg(&staged)
            .output();
    }
    if let Err(e) = ffmpeg::verify_binary(&staged) {
        cleanup(&tmp_zip, &stage);
        return Err(format!("downloaded build failed verification: {e}"));
    }
    let dest_dir = cfg.cache_dir.join(&source.version).join("bin");
    if let Err(e) = std::fs::create_dir_all(&dest_dir) {
        cleanup(&tmp_zip, &stage);
        return Err(format!("cannot create install dir: {e}"));
    }
    let dest = dest_dir.join("ffmpeg");
    rm_rf(&dest);
    if std::fs::rename(&staged, &dest).is_err() {
        if let Err(e) = std::fs::copy(&staged, &dest) {
            cleanup(&tmp_zip, &stage);
            return Err(format!("cannot install verified build: {e}"));
        }
        let _ = std::fs::remove_file(&staged);
    }
    cleanup(&tmp_zip, &stage);
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static N: AtomicU64 = AtomicU64::new(0);

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "framesmith-boot-{}-{}-{}",
            tag,
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn unsuitable_host() -> FfmpegFinding {
        FfmpegFinding {
            ffmpeg_path: None,
            ffprobe_path: None,
            version: None,
            major: None,
            has_video_encoder: false,
            has_audio_encoder: false,
            suitable: false,
            reason: "test: no `ffmpeg` binary found on PATH".to_string(),
        }
    }

    fn suitable_host(path: &Path) -> FfmpegFinding {
        FfmpegFinding {
            ffmpeg_path: Some(path.to_path_buf()),
            ffprobe_path: None,
            version: Some("9.0-test".to_string()),
            major: Some(9),
            has_video_encoder: true,
            has_audio_encoder: true,
            suitable: true,
            reason: "test: suitable".to_string(),
        }
    }

    /// Minimal fake `ffmpeg`: answers `-version`/`-encoders` like a
    /// suitable build and "encodes" by writing a marker to the last arg.
    const FAKE_FFMPEG: &str = "#!/bin/sh\n\
         if [ \"$1\" = \"-version\" ]; then\n\
         \x20 echo \"ffmpeg version 9.0-test Copyright (c) test\"; exit 0\n\
         fi\n\
         if [ \"$1\" = \"-hide_banner\" ]; then\n\
         \x20 echo \"Encoders:\"\n\
         \x20 echo \" V..... libx264   libx264 H.264\"\n\
         \x20 echo \" A..... aac        AAC\"\n\
         \x20 exit 0\n\
         fi\n\
         out=\"\"; for a in \"$@\"; do out=\"$a\"; done\n\
         printf 'FAKE-MP4-MARKER-0123456789' > \"$out\"\n\
         exit 0\n";

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &b in data {
            crc ^= b as u32;
            for _ in 0..8 {
                if crc & 1 == 1 {
                    crc = (crc >> 1) ^ 0xEDB8_8320;
                } else {
                    crc >>= 1;
                }
            }
        }
        !crc
    }

    /// Minimal stored (uncompressed) single-entry zip so tests do not
    /// depend on a `zip` binary.
    fn zip_store_single(name: &str, data: &[u8]) -> Vec<u8> {
        let mut z = Vec::new();
        let crc = crc32(data);
        let central_offset: u32 = 30 + name.len() as u32 + data.len() as u32;
        z.extend_from_slice(&0x0403_4B50u32.to_le_bytes());
        z.extend_from_slice(&20u16.to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes());
        z.extend_from_slice(&crc.to_le_bytes());
        z.extend_from_slice(&(data.len() as u32).to_le_bytes());
        z.extend_from_slice(&(data.len() as u32).to_le_bytes());
        z.extend_from_slice(&(name.len() as u16).to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes());
        z.extend_from_slice(name.as_bytes());
        z.extend_from_slice(data);
        let central_len_pos = z.len();
        z.extend_from_slice(&0x0201_4B50u32.to_le_bytes());
        z.extend_from_slice(&0x031Eu16.to_le_bytes());
        z.extend_from_slice(&20u16.to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes());
        z.extend_from_slice(&crc.to_le_bytes());
        z.extend_from_slice(&(data.len() as u32).to_le_bytes());
        z.extend_from_slice(&(data.len() as u32).to_le_bytes());
        z.extend_from_slice(&(name.len() as u16).to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes());
        // Unix mode lives in the UPPER 16 bits of the external attributes.
        z.extend_from_slice(&((0o100755u32) << 16).to_le_bytes());
        z.extend_from_slice(&0u32.to_le_bytes());
        z.extend_from_slice(name.as_bytes());
        let central_len = (z.len() - central_len_pos) as u32;
        z.extend_from_slice(&0x0605_4B50u32.to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes());
        z.extend_from_slice(&1u16.to_le_bytes());
        z.extend_from_slice(&1u16.to_le_bytes());
        z.extend_from_slice(&central_len.to_le_bytes());
        z.extend_from_slice(&central_offset.to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes());
        z
    }

    struct Fixture {
        _root: PathBuf,
        source: PinSource,
        cache: PathBuf,
    }

    /// A working pinned source served from `file://` with a fresh cache.
    fn fixture(tag: &str) -> Fixture {
        let root = tmp(tag);
        let script = root.join("ffmpeg");
        std::fs::write(&script, FAKE_FFMPEG).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let zip = root.join("ffmpeg-test.zip");
        std::fs::write(
            &zip,
            zip_store_single("ffmpeg", FAKE_FFMPEG.as_bytes()),
        )
        .unwrap();
        let bytes = std::fs::read(&script).unwrap();
        let source = PinSource {
            version: format!("9.0-t-{tag}"),
            url: format!("file://{}", zip.display()),
            sha256: sha256::sha256_hex(&bytes),
            entry: "ffmpeg".to_string(),
        };
        let cache = root.join("cache");
        Fixture {
            _root: root,
            source,
            cache,
        }
    }

    fn cfg_for(fx: &Fixture) -> BootstrapConfig {
        BootstrapConfig {
            auto_download: true,
            offline: false,
            source: Some(fx.source.clone()),
            cache_dir: fx.cache.clone(),
            curl_timeout_secs: 30,
        }
    }

    #[test]
    fn suitable_host_short_circuits_without_download() {
        let fx = fixture("short");
        let host = suitable_host(Path::new("/usr/bin/ffmpeg"));
        let out = ensure_ffmpeg(&host, &cfg_for(&fx));
        match out {
            Bootstrap::Ready {
                ffmpeg_path,
                downloaded,
                announcements,
            } => {
                assert_eq!(ffmpeg_path, Path::new("/usr/bin/ffmpeg"));
                assert!(!downloaded);
                assert!(announcements.is_empty());
            }
            Bootstrap::Refused { message, .. } => panic!("unexpected refusal: {message}"),
        }
        assert!(!fx.cache.exists(), "no cache writes for a host binary");
    }

    #[test]
    fn disabled_refusal_names_config_and_source() {
        let fx = fixture("disabled");
        let mut cfg = cfg_for(&fx);
        cfg.auto_download = false;
        match ensure_ffmpeg(&unsuitable_host(), &cfg) {
            Bootstrap::Refused { code, message } => {
                assert_eq!(code, "E_NO_FFMPEG");
                assert!(message.contains("ffmpeg_auto_download = false"), "{message}");
                assert!(message.contains("framesmith.toml"), "{message}");
                assert!(message.contains(&fx.source.url), "{message}");
                assert!(message.contains("No MP4 was written"), "{message}");
            }
            other => panic!("expected refusal, got {other:?}"),
        }
        assert!(!fx.cache.exists(), "refusal must not fetch anything");
    }

    #[test]
    fn offline_refusal_gives_manual_fix() {
        let fx = fixture("offline");
        let mut cfg = cfg_for(&fx);
        cfg.offline = true;
        match ensure_ffmpeg(&unsuitable_host(), &cfg) {
            Bootstrap::Refused { code, message } => {
                assert_eq!(code, "E_NO_FFMPEG");
                assert!(message.contains("install FFmpeg"), "{message}");
                assert!(message.contains("No MP4 was written"), "{message}");
                assert!(message.contains("framesmith check"), "{message}");
            }
            other => panic!("expected refusal, got {other:?}"),
        }
        assert!(!fx.cache.exists());
    }

    #[test]
    fn no_pin_refuses_with_manual_fix() {
        let fx = fixture("nopin");
        let mut cfg = cfg_for(&fx);
        cfg.source = None;
        match ensure_ffmpeg(&unsuitable_host(), &cfg) {
            Bootstrap::Refused { code, message } => {
                assert_eq!(code, "E_NO_FFMPEG");
                assert!(message.contains("no pinned FFmpeg build"), "{message}");
            }
            other => panic!("expected refusal, got {other:?}"),
        }
    }

    #[test]
    fn file_download_verifies_installs_and_announces() {
        let fx = fixture("dl");
        let cfg = cfg_for(&fx);
        match ensure_ffmpeg(&unsuitable_host(), &cfg) {
            Bootstrap::Ready {
                ffmpeg_path,
                downloaded,
                announcements,
            } => {
                assert!(downloaded);
                assert_eq!(announcements.len(), 1);
                assert!(announcements[0].contains("downloaded pinned FFmpeg"), "{announcements:?}");
                assert!(announcements[0].contains("SHA256 verified"), "{announcements:?}");
                assert_eq!(ffmpeg_path, ffmpeg::cached_binary(&fx.cache, &fx.source.version));
                // The installed binary passes the same bar as host detection.
                let version = ffmpeg::verify_binary(&ffmpeg_path).unwrap();
                assert!(version.starts_with("9.0-test"), "{version}");
                // No temp artifacts leak.
                let leftovers: Vec<_> = std::fs::read_dir(&fx.cache)
                    .unwrap()
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        e.file_name().to_string_lossy().starts_with(".download-")
                            || e.file_name().to_string_lossy().starts_with(".stage-")
                    })
                    .collect();
                assert!(leftovers.is_empty(), "{leftovers:?}");
            }
            Bootstrap::Refused { message, .. } => panic!("unexpected refusal: {message}"),
        }
    }

    #[test]
    fn checksum_mismatch_refuses_and_installs_nothing() {
        let fx = fixture("badsum");
        let mut cfg = cfg_for(&fx);
        cfg.source.as_mut().unwrap().sha256 = "0".repeat(64);
        match ensure_ffmpeg(&unsuitable_host(), &cfg) {
            Bootstrap::Refused { code, message } => {
                assert_eq!(code, "E_DOWNLOAD_FAILED");
                assert!(message.contains("checksum mismatch"), "{message}");
                assert!(message.contains("No MP4 was written"), "{message}");
            }
            other => panic!("expected refusal, got {other:?}"),
        }
        let installed = ffmpeg::cached_binary(&fx.cache, &fx.source.version);
        assert!(!installed.exists(), "a bad build must never be installed");
    }

    #[test]
    fn interrupted_download_refuses_with_cause() {
        let fx = fixture("interrupt");
        let mut cfg = cfg_for(&fx);
        cfg.source.as_mut().unwrap().url = "file:///nonexistent-framesmith-missing.zip".to_string();
        match ensure_ffmpeg(&unsuitable_host(), &cfg) {
            Bootstrap::Refused { code, message } => {
                assert_eq!(code, "E_DOWNLOAD_FAILED");
                assert!(message.contains("download failed"), "{message}");
            }
            other => panic!("expected refusal, got {other:?}"),
        }
    }

    #[test]
    fn cache_reuse_skips_network_and_stale_cache_refreshes() {
        let fx = fixture("reuse");
        let cfg = cfg_for(&fx);
        match ensure_ffmpeg(&unsuitable_host(), &cfg) {
            Bootstrap::Ready { downloaded, .. } => assert!(downloaded),
            Bootstrap::Refused { message, .. } => panic!("unexpected refusal: {message}"),
        }
        // Point the source at a missing archive: reuse must not touch it.
        let mut cached_cfg = cfg.clone();
        cached_cfg.source.as_mut().unwrap().url =
            "file:///nonexistent-framesmith-missing.zip".to_string();
        match ensure_ffmpeg(&unsuitable_host(), &cached_cfg) {
            Bootstrap::Ready {
                downloaded,
                announcements,
                ..
            } => {
                assert!(!downloaded);
                assert!(announcements[0].contains("reusing cached"), "{announcements:?}");
            }
            Bootstrap::Refused { message, .. } => panic!("expected cache reuse: {message}"),
        }
        // Corrupt the cache: the next run re-downloads from the good URL.
        let installed = ffmpeg::cached_binary(&fx.cache, &fx.source.version);
        std::fs::write(&installed, b"garbage").unwrap();
        match ensure_ffmpeg(&unsuitable_host(), &cfg) {
            Bootstrap::Ready { downloaded, .. } => assert!(downloaded, "stale cache must refresh"),
            Bootstrap::Refused { message, .. } => panic!("expected refresh: {message}"),
        }
    }
}
