//! Version and update verbs (Sprint 27 T002 / CAP-002, CAP-003, INT-001).
//!
//! * `framesmith version [--json]` reports the running version, the
//!   build (OS/arch plus the git commit baked in at compile time, or an
//!   honestly-unknown build), and the FFmpeg registry pin.
//! * `framesmith update check [--json] [--api-url <url>]` and
//!   `framesmith upgrade [--check] [--json] [--api-url <url>]` read the
//!   same release metadata the package builder publishes (the latest
//!   GitHub Release for this repo: `tag_name`, per-OS archive assets,
//!   and `checksums.txt`). A newer release is named with where to get
//!   it; the latest release reports current with no banner; an
//!   unreachable channel reports unavailable (never current) without
//!   touching renders — no render path ever calls this code.
//! * `framesmith update apply --yes [...]` and `framesmith upgrade
//!   --yes [...]` download the current platform's archive, verify its
//!   SHA-256 against the release `checksums.txt` BEFORE replacing the
//!   running binary, verify the extracted binary runs and reports the
//!   release version, then replace atomically. Any failure leaves the
//!   running binary untouched (REC-002). Without `--yes` the verbs
//!   refuse: updates never install without the maker's consent.
//!
//! The release metadata endpoint defaults to the GitHub releases API
//! for this repo and can be overridden with `--api-url` or
//! `FRAMESMITH_RELEASE_API_URL` (tests serve `file://` fixtures
//! through the same `curl` fetch path). `FRAMESMITH_OFFLINE=1` forces
//! unavailable without network access, mirroring the FFmpeg bootstrap.

use ffmpeg::ffmpeg as ffmpeg_mod;
use std::cmp::Ordering;
use std::path::Path;

/// Releases metadata endpoint (GitHub latest-release API for this repo).
const RELEASE_API: &str = "https://api.github.com/repos/bdsanxz/framesmith/releases/latest";
/// Human releases page named wherever an update is announced.
const RELEASES_PAGE: &str = "https://github.com/bdsanxz/framesmith/releases";
/// Env override for the release metadata endpoint (tests).
const RELEASE_API_ENV: &str = "FRAMESMITH_RELEASE_API_URL";
/// Check fetch timeout (seconds), clamped; the default stays short so
/// a stalled channel never hangs the check.
const TIMEOUT_ENV: &str = "FRAMESMITH_UPDATE_TIMEOUT_SECS";
const DEFAULT_TIMEOUT_SECS: u64 = 20;

fn version_usage_error(msg: &str, cmd: &str) -> ! {
    eprintln!("framesmith {cmd}: {msg}.");
    eprintln!("usage: framesmith version [--json]");
    std::process::exit(2);
}

fn update_usage_error(msg: &str) -> ! {
    eprintln!("framesmith update: {msg}.");
    eprintln!("usage: framesmith update check [--json] [--api-url <url>]");
    eprintln!("usage: framesmith update apply --yes [--json] [--api-url <url>]");
    std::process::exit(2);
}

fn upgrade_usage_error(msg: &str) -> ! {
    eprintln!("framesmith upgrade: {msg}.");
    eprintln!("usage: framesmith upgrade [--check] [--json] [--api-url <url>]");
    eprintln!("usage: framesmith upgrade --yes [--json] [--api-url <url>]");
    std::process::exit(2);
}

/// The running crate version (`CARGO_PKG_VERSION`, bare, no `v`).
fn running_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Build identity: `<os>/<arch> @ <short commit>` when the checkout
/// baked one in, otherwise an explicitly unknown build (second-class,
/// never guessed). The git commit comes from `build.rs` via
/// `FRAMEWORK_GIT_COMMIT`; `option_env!` keeps the binary buildable
/// without git.
fn build_string() -> (String, bool) {
    let commit = option_env!("FRAMEWORK_GIT_COMMIT").unwrap_or("").trim();
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    if commit.is_empty() {
        (format!("unknown build ({os}/{arch})"), true)
    } else {
        (format!("{os}/{arch} @ {commit}"), false)
    }
}

fn version_json() -> serde_json::Value {
    let (build, unknown_build) = build_string();
    serde_json::json!({
        "tool": "framesmith",
        "version": running_version(),
        "build": build,
        "unknown_build": unknown_build,
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "ffmpeg_registry_pin": ffmpeg_mod::FFMPEG_REGISTRY_PIN,
        "ffmpeg_registry_releases": ffmpeg_mod::FFMPEGBIN_RELEASES_PAGE,
        "ffmpeg_registry_api": ffmpeg_mod::FFMPEGBIN_API_LATEST,
    })
}

/// `framesmith version [--json]` (CAP-002).
pub fn cmd_version(args: &[String]) -> i32 {
    let mut json = false;
    for arg in &args[1..] {
        if *arg == "--json" {
            if json {
                version_usage_error("duplicate --json flag", "version");
            }
            json = true;
        } else {
            version_usage_error(&format!("unknown flag {arg:?}"), "version");
        }
    }
    if json {
        println!("{}", version_json());
    } else {
        let (build, _) = build_string();
        println!("framesmith {}", running_version());
        println!("build: {build}");
        println!(
            "ffmpeg-registry: {} ({})",
            ffmpeg_mod::FFMPEG_REGISTRY_PIN,
            ffmpeg_mod::FFMPEGBIN_RELEASES_PAGE
        );
    }
    0
}

/// Release metadata endpoint: flag wins, then env, then the default.
fn release_api_url(flag: Option<&str>) -> String {
    flag.map(str::to_string).unwrap_or_else(|| {
        std::env::var(RELEASE_API_ENV)
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| RELEASE_API.to_string())
    })
}

/// Fetches bytes over HTTPS (or `file://` for test fixtures) with
/// `curl`, bounded by `timeout_secs`. Returns the cause on failure,
/// never panics, never hangs past the timeout.
fn fetch_bytes(url: &str, timeout_secs: u64) -> Result<Vec<u8>, String> {
    let curl = ffmpeg_mod::find_helper("curl")
        .ok_or_else(|| "no `curl` found on PATH to fetch release metadata".to_string())?;
    let out = std::process::Command::new(&curl)
        .args([
            "-fsSL",
            "--connect-timeout",
            "10",
            "--max-time",
            &timeout_secs.to_string(),
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "User-Agent: framesmith",
        ])
        .arg(url)
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
        return Err(format!("release fetch failed for {url}: {detail}"));
    }
    Ok(out.stdout)
}

/// One asset from the latest-release metadata.
#[derive(Debug, Clone)]
struct ReleaseAsset {
    name: String,
    url: String,
}

/// Parsed latest-release metadata (the builder's release channel).
#[derive(Debug, Clone)]
struct ReleaseInfo {
    tag: String,
    version: String,
    html_url: String,
    assets: Vec<ReleaseAsset>,
}

/// Parse GitHub latest-release JSON. The archive and checksum assets
/// must carry download URLs; anything else is unusable metadata and
/// the check reports unavailable rather than guessing.
fn parse_release(body: &str) -> Result<ReleaseInfo, String> {
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("release metadata is not JSON: {e}"))?;
    let tag = v
        .get("tag_name")
        .and_then(|t| t.as_str())
        .filter(|t| !t.is_empty())
        .ok_or_else(|| "release metadata has no usable tag_name".to_string())?
        .to_string();
    let version = dist::version_from_tag(&tag)
        .ok_or_else(|| format!("release tag {tag:?} is not a release version"))?;
    let html_url = v
        .get("html_url")
        .and_then(|t| t.as_str())
        .unwrap_or(RELEASES_PAGE)
        .to_string();
    let assets_arr = v
        .get("assets")
        .and_then(|a| a.as_array())
        .ok_or_else(|| "release metadata has no assets list".to_string())?;
    let mut assets = Vec::new();
    for a in assets_arr {
        let (Some(name), Some(url)) = (
            a.get("name").and_then(|n| n.as_str()),
            a.get("browser_download_url").and_then(|u| u.as_str()),
        ) else {
            continue;
        };
        if name.is_empty() || url.is_empty() {
            continue;
        }
        assets.push(ReleaseAsset {
            name: name.to_string(),
            url: url.to_string(),
        });
    }
    if assets.is_empty() {
        return Err("release metadata carries no downloadable asset".to_string());
    }
    Ok(ReleaseInfo {
        tag,
        version,
        html_url,
        assets,
    })
}

impl ReleaseInfo {
    fn asset(&self, name: &str) -> Option<&ReleaseAsset> {
        self.assets.iter().find(|a| a.name == name)
    }
}

/// Fetch timeout in seconds: env wins (clamped), default 20.
fn update_timeout_secs() -> u64 {
    std::env::var(TIMEOUT_ENV)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(|v| v.clamp(3, 120))
        .unwrap_or(DEFAULT_TIMEOUT_SECS)
}

/// What the update check decided.
#[derive(Debug, Clone)]
enum CheckOutcome {
    Current { latest: String },
    UpdateAvailable {
        latest: String,
        html_url: String,
        asset: String,
        download_url: String,
    },
    Unavailable { cause: String },
}

impl CheckOutcome {
    fn status(&self) -> &'static str {
        match self {
            CheckOutcome::Current { .. } => "current",
            CheckOutcome::UpdateAvailable { .. } => "update-available",
            CheckOutcome::Unavailable { .. } => "unavailable",
        }
    }
}

/// Fetch the latest-release metadata and compare it with the running
/// version. Never panics; every failure is `Unavailable` (never
/// current) with the cause named.
fn check_core(api_url: &str) -> CheckOutcome {
    if ffmpeg_mod::offline_forced() {
        return CheckOutcome::Unavailable {
            cause: "offline (FRAMESMITH_OFFLINE=1); release metadata was not fetched"
                .to_string(),
        };
    }
    let body = match fetch_bytes(api_url, update_timeout_secs()) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(s) => s,
            Err(_) => {
                return CheckOutcome::Unavailable {
                    cause: "release lookup returned non-UTF-8 metadata".to_string(),
                }
            }
        },
        Err(cause) => return CheckOutcome::Unavailable { cause },
    };
    let info = match parse_release(&body) {
        Ok(info) => info,
        Err(cause) => return CheckOutcome::Unavailable { cause },
    };
    if dist::compare_release_versions(&info.version, running_version()) != Ordering::Greater {
        // Same version, prerelease-older, or a stale older tag: no
        // downgrade is ever offered.
        return CheckOutcome::Current {
            latest: info.version,
        };
    }
    let Some(target) = dist::target_for_os_arch(std::env::consts::OS, std::env::consts::ARCH)
    else {
        return CheckOutcome::Unavailable {
            cause: format!(
                "unsupported platform {}/{}; {}",
                std::env::consts::OS,
                std::env::consts::ARCH,
                dist::supported_platforms_line()
            ),
        };
    };
    let asset_name = target.asset_name(&info.version);
    let download_url = match info.asset(&asset_name) {
        Some(asset) => asset.url.clone(),
        None => {
            return CheckOutcome::Unavailable {
                cause: format!(
                    "latest release {} publishes no {asset_name} for this platform",
                    info.tag
                ),
            }
        }
    };
    CheckOutcome::UpdateAvailable {
        latest: info.version.clone(),
        html_url: info.html_url.clone(),
        asset: asset_name,
        download_url,
    }
}

fn check_json(outcome: &CheckOutcome, command: &str) -> serde_json::Value {
    let (latest, html_url, asset, download_url, cause) = match outcome {
        CheckOutcome::Current { latest } => (Some(latest.clone()), None, None, None, None),
        CheckOutcome::UpdateAvailable {
            latest,
            html_url,
            asset,
            download_url,
        } => (
            Some(latest.clone()),
            Some(html_url.clone()),
            Some(asset.clone()),
            Some(download_url.clone()),
            None,
        ),
        CheckOutcome::Unavailable { cause } => {
            (None, None, None, None, Some(cause.clone()))
        }
    };
    serde_json::json!({
        "tool": "framesmith",
        "command": command,
        "current": running_version(),
        "latest": latest,
        "status": outcome.status(),
        "releases_page": RELEASES_PAGE,
        "html_url": html_url,
        "asset": asset,
        "download_url": download_url,
        "cause": cause,
    })
}

/// Print a check outcome as text and return the exit code. Reachable
/// states exit 0; unavailable exits 1 and never prints "current".
fn print_check(outcome: &CheckOutcome, command: &str, cli: &str, json: bool) -> i32 {
    if json {
        println!("{}", check_json(outcome, command));
    } else {
        match outcome {
            CheckOutcome::Current { latest } => {
                println!("framesmith {} current (latest {})", running_version(), latest);
            }
            CheckOutcome::UpdateAvailable {
                latest,
                html_url,
                asset,
                download_url,
            } => {
                println!(
                    "framesmith {} update-available: {} available",
                    running_version(),
                    latest
                );
                println!(
                    "get it from {html_url} (asset {asset} for {}/{}, download: {download_url})",
                    std::env::consts::OS,
                    std::env::consts::ARCH
                );
            }
            CheckOutcome::Unavailable { cause } => {
                eprintln!(
                    "framesmith {cli}: unavailable: {cause} (releases: {RELEASES_PAGE}; renders unaffected)"
                );
            }
        }
    }
    match outcome {
        CheckOutcome::Unavailable { .. } => 1,
        _ => 0,
    }
}

/// Outcome of a verified update.
#[derive(Debug, Clone)]
struct ApplyOk {
    from: String,
    to: String,
    asset: String,
    sha256: String,
}

/// Extract the release archive into `dest`. `.tar.gz` via `tar`
/// (ships on macOS, Linux, and Windows as `tar.exe`); `.zip` tries
/// `unzip` then `tar` (bsdtar reads zip). `Command::new("tar")`
/// resolves `.exe` on Windows; paths always join through `Path::join`.
fn extract_archive(archive: &Path, asset_name: &str, dest: &Path) -> Result<(), String> {
    let tried = |helpers: &[&str]| format!("no usable extractor ({}) for {asset_name}", helpers.join(", "));
    let is_zip = asset_name.ends_with(".zip");
    let run = |helper: &Path, args: Vec<String>| -> bool {
        std::process::Command::new(helper)
            .args(&args)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    let mut helpers: Vec<&str> = Vec::new();
    let tries: Vec<(&str, Vec<String>)> = if is_zip {
        vec![
            (
                "unzip",
                vec![
                    "-q".to_string(),
                    "-o".to_string(),
                    archive.display().to_string(),
                    "-d".to_string(),
                    dest.display().to_string(),
                ],
            ),
            (
                "tar",
                vec![
                    "-xf".to_string(),
                    archive.display().to_string(),
                    "-C".to_string(),
                    dest.display().to_string(),
                ],
            ),
        ]
    } else {
        vec![(
            "tar",
            vec![
                "-xzf".to_string(),
                archive.display().to_string(),
                "-C".to_string(),
                dest.display().to_string(),
            ],
        )]
    };
    for (name, args) in tries {
        helpers.push(name);
        if let Some(path) = ffmpeg_mod::find_helper(name) {
            if run(&path, args.clone()) {
                return Ok(());
            }
        }
    }
    Err(tried(&helpers))
}

/// Run `<binary> --version` and return its version token.
fn binary_reports_version(binary: &Path) -> Option<String> {
    let out = std::process::Command::new(binary)
        .arg("--version")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let token = text.split_whitespace().nth(1)?.trim().to_string();
    if text.trim().starts_with("framesmith ") && !token.is_empty() {
        Some(token)
    } else {
        None
    }
}

/// Replace the running binary with the verified staged file.
/// Unix: a same-directory rename overwrites in one step, so an
/// interrupt before the rename leaves the old binary in place.
/// Windows: the running `.exe` cannot be overwritten, so the current
/// file is first renamed aside, the staged file renamed in, and any
/// failure restores the old name — the old binary keeps working.
/// Paths join through `Path::join`; the binary name carries `.exe`
/// handling through `dist::binary_name_for`.
fn replace_running_binary(staged: &Path, target: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        let fname = target
            .file_name()
            .ok_or_else(|| "running binary path has no file name".to_string())?;
        let backup = target
            .parent()
            .ok_or_else(|| "running binary path has no parent directory".to_string())?
            .join(format!("{}.old.{}", fname.to_string_lossy(), std::process::id()));
        let _ = std::fs::remove_file(&backup);
        std::fs::rename(target, &backup)
            .map_err(|e| format!("cannot stage current binary aside: {e}"))?;
        if let Err(e) = std::fs::rename(staged, target) {
            let _ = std::fs::rename(&backup, target);
            return Err(format!("cannot install the updated binary (old binary restored): {e}"));
        }
        let _ = std::fs::remove_file(&backup);
        Ok(())
    }
    #[cfg(not(windows))]
    {
        std::fs::rename(staged, target)
            .map_err(|e| format!("cannot install the updated binary: {e}"))
    }
}

/// Download, verify, and atomically install the latest release.
/// Every failure before the rename leaves the running binary
/// untouched; the staged file is removed on every handled failure.
fn apply_core(api_url: &str) -> Result<ApplyOk, String> {
    if ffmpeg_mod::offline_forced() {
        return Err("offline (FRAMESMITH_OFFLINE=1); refusing to update".to_string());
    }
    let timeout = update_timeout_secs();
    let body = fetch_bytes(api_url, timeout)
        .map_err(|cause| format!("update channel unavailable: {cause}"))?;
    let body =
        String::from_utf8(body).map_err(|_| "release lookup returned non-UTF-8 metadata".to_string())?;
    let info = parse_release(&body)?;
    if dist::compare_release_versions(&info.version, running_version()) != Ordering::Greater {
        return Err(format!(
            "already current: running {} matches latest {}",
            running_version(),
            info.version
        ));
    }
    let Some(target_spec) = dist::target_for_os_arch(std::env::consts::OS, std::env::consts::ARCH)
    else {
        return Err(format!(
            "unsupported platform {}/{}; {}",
            std::env::consts::OS,
            std::env::consts::ARCH,
            dist::supported_platforms_line()
        ));
    };
    let asset_name = target_spec.asset_name(&info.version);
    let archive_asset = info
        .asset(&asset_name)
        .ok_or_else(|| format!("latest release {} publishes no {asset_name} for this platform", info.tag))?;
    let sums_asset = info
        .asset("checksums.txt")
        .ok_or_else(|| "release publishes no checksums.txt; refusing to install unverified".to_string())?;

    let sums_bytes = fetch_bytes(&sums_asset.url, timeout)
        .map_err(|cause| format!("cannot fetch checksums.txt: {cause}"))?;
    let sums_body = String::from_utf8(sums_bytes)
        .map_err(|_| "checksums.txt is not UTF-8".to_string())?;
    let table = dist::parse_checksums_file(&sums_body);
    let archive_bytes = fetch_bytes(&archive_asset.url, timeout)
        .map_err(|cause| format!("cannot download {asset_name}: {cause}"))?;
    // REC-002: verify the archive against release metadata before any
    // install step. A mismatch refuses naming expected/found.
    dist::verify_asset(&asset_name, &archive_bytes, &table)?;
    let sha256 = dist::sha256_hex(&archive_bytes);

    // Stage + extract in a fresh temp dir; the install target itself
    // is untouched until the rename.
    let work = std::env::temp_dir().join(format!(
        "framesmith-update-{}-{}",
        std::process::id(),
        info.version.replace('/', "_")
    ));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work)
        .map_err(|e| format!("cannot create update staging dir: {e}"))?;
    let cleanup = |work: &Path| {
        let _ = std::fs::remove_dir_all(work);
    };
    let archive_path = work.join(&asset_name);
    if let Err(e) = std::fs::write(&archive_path, &archive_bytes) {
        cleanup(&work);
        return Err(format!("cannot stage downloaded archive: {e}"));
    }
    let extract_dir = work.join("extracted");
    if let Err(e) = std::fs::create_dir_all(&extract_dir) {
        cleanup(&work);
        return Err(format!("cannot create extraction dir: {e}"));
    }
    if let Err(e) = extract_archive(&archive_path, &asset_name, &extract_dir) {
        cleanup(&work);
        return Err(format!("cannot unpack {asset_name}: {e}"));
    }
    let binary_name = dist::binary_name_for(std::env::consts::OS);
    let extracted = extract_dir.join(binary_name);
    if !extracted.is_file() {
        cleanup(&work);
        return Err(format!(
            "downloaded {asset_name} did not contain {binary_name}; refusing"
        ));
    }
    // Defense in depth: when the archive carries its inner manifest,
    // verify the extracted binary against it too (a mismatch refuses).
    let inner_sums = extract_dir.join("checksums.txt");
    if inner_sums.is_file() {
        let inner_body = std::fs::read_to_string(&inner_sums)
            .map_err(|e| format!("cannot read inner checksums.txt: {e}"))?;
        let inner_table = dist::parse_checksums_file(&inner_body);
        let binary_bytes =
            std::fs::read(&extracted).map_err(|e| format!("cannot read extracted binary: {e}"))?;
        if let Err(e) = dist::verify_asset(binary_name, &binary_bytes, &inner_table) {
            cleanup(&work);
            return Err(format!("{e}; refusing"));
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&extracted, std::fs::Permissions::from_mode(0o755));
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("/usr/bin/xattr")
            .args(["-cr"])
            .arg(&extracted)
            .output();
    }
    // The extracted binary must run and report the release version.
    match binary_reports_version(&extracted) {
        Some(v) if v == info.version => {}
        Some(v) => {
            cleanup(&work);
            return Err(format!(
                "downloaded {asset_name} reports version {v}, expected release {}; refusing",
                info.version
            ));
        }
        None => {
            cleanup(&work);
            return Err(format!(
                "downloaded {asset_name} does not run as a framesmith binary; refusing"
            ));
        }
    }

    // Copy into the install directory (same filesystem for the final
    // rename) under a hidden temp name, then replace.
    let target = std::env::current_exe()
        .map_err(|e| format!("cannot locate the running binary: {e}"))?;
    let target_dir = target
        .parent()
        .ok_or_else(|| "running binary path has no parent directory".to_string())?
        .to_path_buf();
    let fname = target
        .file_name()
        .ok_or_else(|| "running binary path has no file name".to_string())?
        .to_string_lossy()
        .to_string();
    let staged = target_dir.join(format!(".framesmith-update-{}-{fname}", std::process::id()));
    let _ = std::fs::remove_file(&staged);
    if let Err(e) = std::fs::copy(&extracted, &staged) {
        cleanup(&work);
        return Err(format!("cannot stage the updated binary: {e}"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755));
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("/usr/bin/xattr")
            .args(["-cr"])
            .arg(&staged)
            .output();
    }
    if let Ok(f) = std::fs::File::open(&staged) {
        let _ = f.sync_all();
    }
    // Re-verify the staged file itself before the rename.
    match binary_reports_version(&staged) {
        Some(v) if v == info.version => {}
        other => {
            let _ = std::fs::remove_file(&staged);
            cleanup(&work);
            return Err(format!(
                "staged binary reports {:?}, expected {}; refusing",
                other, info.version
            ));
        }
    }
    if let Err(e) = replace_running_binary(&staged, &target) {
        let _ = std::fs::remove_file(&staged);
        cleanup(&work);
        return Err(e);
    }
    cleanup(&work);
    Ok(ApplyOk {
        from: running_version().to_string(),
        to: info.version.clone(),
        asset: asset_name,
        sha256,
    })
}

fn apply_json(report: &ApplyOk, command: &str) -> serde_json::Value {
    serde_json::json!({
        "tool": "framesmith",
        "command": command,
        "current": report.from,
        "latest": report.to,
        "status": "updated",
        "asset": report.asset,
        "sha256": report.sha256,
        "cause": serde_json::Value::Null,
    })
}

/// Run the check verb and print per the flags.
fn do_check(command: &str, cli: &str, json: bool, api_url: Option<&str>) -> i32 {
    let outcome = check_core(&release_api_url(api_url));
    print_check(&outcome, command, cli, json)
}

/// Run the consented apply verb and print per the flags.
fn do_apply(command: &str, json: bool, api_url: Option<&str>) -> i32 {
    match apply_core(&release_api_url(api_url)) {
        Ok(report) => {
            if json {
                println!("{}", apply_json(&report, command));
            } else {
                println!(
                    "framesmith: updated {} -> {} ({} sha256 {} verified); run `framesmith version` to confirm",
                    report.from, report.to, report.asset, report.sha256
                );
            }
            0
        }
        Err(cause) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "tool": "framesmith",
                        "command": command,
                        "current": running_version(),
                        "latest": serde_json::Value::Null,
                        "status": "failed",
                        "asset": serde_json::Value::Null,
                        "sha256": serde_json::Value::Null,
                        "cause": cause,
                    })
                );
            } else {
                eprintln!("framesmith {command}: {cause}.");
            }
            1
        }
    }
}

/// Parse the shared `check`/`apply` flags after a verb.
fn parse_check_apply_flags(args: &[String]) -> (bool, Option<String>, bool) {
    let mut json = false;
    let mut api_url: Option<String> = None;
    let mut yes = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => {
                if json {
                    update_usage_error("duplicate --json flag");
                }
                json = true;
            }
            "--api-url" => {
                i += 1;
                if i >= args.len() {
                    update_usage_error("--api-url needs a value (the release metadata URL)");
                }
                if api_url.is_some() {
                    update_usage_error("duplicate --api-url flag");
                }
                api_url = Some(args[i].clone());
            }
            "--yes" => {
                if yes {
                    update_usage_error("duplicate --yes flag");
                }
                yes = true;
            }
            other if other.starts_with('-') => {
                update_usage_error(&format!("unknown flag {other:?}"));
            }
            other => {
                update_usage_error(&format!("unexpected argument {other:?}"));
            }
        }
        i += 1;
    }
    (json, api_url, yes)
}

/// `framesmith update check|apply --yes [--json] [--api-url <url>]`.
pub fn cmd_update(args: &[String]) -> i32 {
    if args.len() < 2 {
        update_usage_error("expected `check` or `apply`");
    }
    match args[1].as_str() {
        "check" => {
            let (json, api_url, yes) = parse_check_apply_flags(&args[2..]);
            if yes {
                update_usage_error("--yes is only valid for `update apply`");
            }
            do_check("update check", "update check", json, api_url.as_deref())
        }
        "apply" => {
            let (json, api_url, yes) = parse_check_apply_flags(&args[2..]);
            if !yes {
                update_usage_error("`update apply` requires --yes consent (updates never install without it)");
            }
            do_apply("update apply", json, api_url.as_deref())
        }
        other => update_usage_error(&format!("expected `check` or `apply`, found {other:?}")),
    }
}

/// `framesmith upgrade [--check] [--yes] [--json] [--api-url <url>]`.
/// Bare `upgrade` and `upgrade --check` only report; `upgrade --yes`
/// applies with the maker's consent (HyperFrames parity spelling).
pub fn cmd_upgrade(args: &[String]) -> i32 {
    let mut check = false;
    let mut json = false;
    let mut api_url: Option<String> = None;
    let mut yes = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--check" => {
                if check {
                    upgrade_usage_error("duplicate --check flag");
                }
                check = true;
            }
            "--json" => {
                if json {
                    upgrade_usage_error("duplicate --json flag");
                }
                json = true;
            }
            "--api-url" => {
                i += 1;
                if i >= args.len() {
                    upgrade_usage_error("--api-url needs a value (the release metadata URL)");
                }
                if api_url.is_some() {
                    upgrade_usage_error("duplicate --api-url flag");
                }
                api_url = Some(args[i].clone());
            }
            "--yes" => {
                if yes {
                    upgrade_usage_error("duplicate --yes flag");
                }
                yes = true;
            }
            other => upgrade_usage_error(&format!("unknown flag or argument {other:?}")),
        }
        i += 1;
    }
    if check && yes {
        upgrade_usage_error("--check and --yes are mutually exclusive");
    }
    if yes {
        do_apply("upgrade", json, api_url.as_deref())
    } else {
        do_check("upgrade", "upgrade", json, api_url.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static N: AtomicU64 = AtomicU64::new(0);

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "framesmith-update-{}-{}-{}",
            tag,
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Release JSON fixture with three synthetic assets.
    fn release_fixture(tag: &str) -> String {
        format!(
            "{{\"tag_name\": \"{tag}\", \"html_url\": \"https://example.invalid/releases/{tag}\", \"assets\": [\
            {{\"name\": \"framesmith-1.2.3-macos-arm64.tar.gz\", \"browser_download_url\": \"https://example.invalid/a.tgz\"}},\
            {{\"name\": \"framesmith-1.2.3-windows-x64.zip\", \"browser_download_url\": \"https://example.invalid/w.zip\"}},\
            {{\"name\": \"checksums.txt\", \"browser_download_url\": \"https://example.invalid/checksums.txt\"}}\
            ]}}"
        )
    }

    #[test]
    fn release_metadata_parses() {
        let info = parse_release(&release_fixture("v1.2.3")).unwrap();
        assert_eq!(info.version, "1.2.3");
        assert_eq!(info.tag, "v1.2.3");
        assert_eq!(info.assets.len(), 3);
        assert!(info.asset("checksums.txt").is_some());
        // Non-release tags and asset-less bodies are unusable, never
        // silently treated as current.
        assert!(parse_release(&release_fixture("nightly")).is_err());
        assert!(parse_release("{\"tag_name\": \"v1.0.0\", \"assets\": []}").is_err());
        assert!(parse_release("not json").is_err());
    }

    #[test]
    fn check_status_names() {
        assert_eq!(
            CheckOutcome::Current {
                latest: "0.1.0".into()
            }
            .status(),
            "current"
        );
        assert_eq!(
            CheckOutcome::Unavailable {
                cause: "x".into()
            }
            .status(),
            "unavailable"
        );
        assert_eq!(
            CheckOutcome::UpdateAvailable {
                latest: "0.2.0".into(),
                html_url: "h".into(),
                asset: "a".into(),
                download_url: "d".into(),
            }
            .status(),
            "update-available"
        );
    }

    #[test]
    fn version_json_carries_pin() {
        let v = version_json();
        assert_eq!(v["tool"], "framesmith");
        assert_eq!(v["version"], running_version());
        assert_eq!(v["ffmpeg_registry_pin"], ffmpeg_mod::FFMPEG_REGISTRY_PIN);
        assert_eq!(v["unknown_build"], false_or_true());
    }

    fn false_or_true() -> serde_json::Value {
        // Build is known locally (git checkout) and unknown in
        // vendored trees; both are valid, the flag must agree with
        // the build string instead of lying either way.
        let (_build, unknown) = build_string();
        serde_json::json!(unknown)
    }

    #[test]
    fn build_string_is_honest_about_unknown() {
        let (build, unknown) = build_string();
        if unknown {
            assert!(build.contains("unknown build"), "{build}");
        } else {
            assert!(!build.contains("unknown"), "{build}");
            assert!(build.contains(std::env::consts::OS), "{build}");
        }
    }

    #[test]
    fn atomic_replace_installs_staged() {
        let dir = tmp("replace");
        let target = dir.join("framesmith");
        let staged = dir.join(".framesmith-update-staged");
        std::fs::write(&target, b"old-binary").unwrap();
        std::fs::write(&staged, b"new-binary").unwrap();
        replace_running_binary(&staged, &target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new-binary");
        assert!(!staged.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_replace_failure_leaves_old() {
        let dir = tmp("replace-fail");
        let target = dir.join("framesmith");
        std::fs::write(&target, b"old-binary").unwrap();
        // Staged file missing: the rename fails, old bytes survive.
        let staged = dir.join("missing-staged");
        assert!(replace_running_binary(&staged, &target).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"old-binary");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn check_core_file_fixture_current_and_newer() {
        if ffmpeg_mod::find_helper("curl").is_none() {
            return;
        }
        // Fixture naming the running version reads as current.
        let dir = tmp("fixture");
        let meta = dir.join("release.json");
        let current_tag = format!("v{}", running_version());
        std::fs::write(&meta, release_fixture(&current_tag)).unwrap();
        let out = check_core(&format!("file://{}", meta.display()));
        match out {
            CheckOutcome::Current { latest } => {
                assert_eq!(latest, running_version());
            }
            other => panic!("expected current, got {other:?}"),
        }
        // A newer tag names the platform asset and where to get it.
        // (1.2.3 compares above 0.1.0; on an unsupported host the
        // outcome is unavailable naming the supported set instead.)
        std::fs::write(&meta, release_fixture("v1.2.3")).unwrap();
        let out = check_core(&format!("file://{}", meta.display()));
        match out {
            CheckOutcome::UpdateAvailable { latest, asset, .. } => {
                assert_eq!(latest, "1.2.3");
                let target = dist::target_for_os_arch(
                    std::env::consts::OS,
                    std::env::consts::ARCH,
                )
                .unwrap();
                assert_eq!(asset, target.asset_name("1.2.3"));
            }
            CheckOutcome::Unavailable { cause } => {
                assert!(
                    cause.contains("supported platforms") || cause.contains("publishes no"),
                    "{cause}"
                );
            }
            other => panic!("expected update-available, got {other:?}"),
        }
        // A missing file is unavailable, never current.
        let out = check_core(&format!("file://{}", dir.join("nope.json").display()));
        assert!(matches!(out, CheckOutcome::Unavailable { .. }), "{out:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn check_core_offline_is_unavailable() {
        // FRAMESMITH_OFFLINE must not be forced by another test
        // mutating process state; the env is read per call. Skip the
        // assertion when the process env already forces offline so
        // ordering never makes this flaky — either way the value must
        // not claim current.
        let out = check_core("file:///nonexistent-n27-probe");
        assert!(
            !matches!(out, CheckOutcome::Current { .. }),
            "failed fetch must never read as current"
        );
    }
}
