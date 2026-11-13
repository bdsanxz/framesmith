//! Per-OS installer packaging (Sprint 27, T001 / CAP-001).
//!
//! The package builder assembles installable packages from tagged releases
//! for four OS/arch pairs: macOS arm64/x64, Windows x64, Linux x64. Each
//! package ships the binary plus launchers and third-party notices,
//! verifies checksums against release metadata before install, and installs
//! with one command per OS. The binary renders immediately through the
//! existing FFmpeg bootstrap — packaging adds no new render path.
//!
//! Boundary: packages build only from tagged releases; an incomplete
//! three-OS set (here four targets, counting macOS arm64+x64 separately)
//! refuses naming the missing OS.
//!
//! Windows handling: the installed binary is `framesmith.exe` on Windows
//! and `framesmith` elsewhere. `Command::new("ffmpeg")` resolves `.exe`
//! on Windows automatically; only explicit `is_file()` PATH joins need
//! the `.exe` suffix — the installers mirror the `ffmpeg` crate's
//! `exe_name()` contract.

/// One shippable target: OS/arch pair, release slug, asset name template,
/// installed binary name, and installer script name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    /// Lowercase OS: `macos`, `windows`, `linux`.
    pub os: &'static str,
    /// Lowercase arch: `arm64`, `x64`.
    pub arch: &'static str,
    /// Human slug used in asset names and error messages:
    /// `macos-arm64`, `macos-x64`, `windows-x64`, `linux-x64`.
    pub slug: &'static str,
    /// Asset file name for a given release version (e.g. `0.1.0`).
    pub asset: &'static str,
    /// Installed binary file name (`framesmith.exe` on Windows).
    pub binary: &'static str,
    /// Installer script that ships in the package.
    pub installer: &'static str,
}

impl Target {
    /// Asset file name for a release version.
    pub fn asset_name(&self, version: &str) -> String {
        format!("framesmith-{}-{}.{}", version, self.slug, self.archive_ext())
    }

    /// Archive extension: `.zip` for Windows, `.tar.gz` elsewhere.
    pub fn archive_ext(&self) -> &'static str {
        if self.os == "windows" {
            "zip"
        } else {
            "tar.gz"
        }
    }
}

/// Every shippable target. The release is incomplete if any one is missing —
/// the failure names the missing slug(s) (REC-001).
pub const REQUIRED_TARGETS: &[Target] = &[
    Target {
        os: "macos",
        arch: "arm64",
        slug: "macos-arm64",
        asset: "",
        binary: "framesmith",
        installer: "install.sh",
    },
    Target {
        os: "macos",
        arch: "x64",
        slug: "macos-x64",
        asset: "",
        binary: "framesmith",
        installer: "install.sh",
    },
    Target {
        os: "windows",
        arch: "x64",
        slug: "windows-x64",
        asset: "",
        binary: "framesmith.exe",
        installer: "install.ps1",
    },
    Target {
        os: "linux",
        arch: "x64",
        slug: "linux-x64",
        asset: "",
        binary: "framesmith",
        installer: "install.sh",
    },
];

/// Find the required target for an OS/arch pair (`macos`/`aarch64` or
/// `macos`/`arm64` spellings are both accepted for macOS; `x86_64` mirrors
/// `x64`). Returns `None` for unsupported combinations — the caller refuses
/// naming the supported set.
pub fn target_for_os_arch(os: &str, arch: &str) -> Option<&'static Target> {
    let norm_arch = match arch {
        "aarch64" | "arm64" => "aarch64",
        "x86_64" | "x64" => "x86_64",
        _other => return None,
    };
    // macOS aarch64 maps to macos-arm64; everything else is exact.
    REQUIRED_TARGETS.iter().find(|t| {
        let t_arch_norm = match t.arch {
            "arm64" => "aarch64",
            "x64" => "x86_64",
            other => other,
        };
        t.os == os && t_arch_norm == norm_arch
    })
}

/// Installed binary name for this OS (`framesmith.exe` on Windows).
/// Kept in sync with `ffmpeg::exe_name()`'s contract.
pub fn binary_name_for(os: &str) -> &'static str {
    if os == "windows" {
        "framesmith.exe"
    } else {
        "framesmith"
    }
}

/// Installer script name for this OS.
pub fn installer_for(os: &str) -> &'static str {
    if os == "windows" {
        "install.ps1"
    } else {
        "install.sh"
    }
}

/// The supported-set wording every refusal names, so an unknown OS/arch
/// pair lists what this build ships.
pub fn supported_platforms_line() -> String {
    let names: Vec<String> = REQUIRED_TARGETS
        .iter()
        .map(|t| format!("{}/{}: framesmith-{{version}}-{}", t.os, t.arch, t.slug))
        .collect();
    format!("supported platforms: {}", names.join("; "))
}

/// Check a complete OS set: each required asset must be present for the
/// release version. Returns the list of missing slugs on failure so the
/// caller refuses naming every missing OS (REC-001). Empty `present`
/// means nothing was built.
pub fn missing_targets(present_assets: &[String], version: &str) -> Vec<&'static str> {
    REQUIRED_TARGETS
        .iter()
        .filter(|t| {
            let want = t.asset_name(version);
            !present_assets.iter().any(|a| a == &want)
        })
        .map(|t| t.slug)
        .collect()
}

/// Verify a complete set; on failure return an error naming every missing
/// OS so the release refuses with the missing set spelled out.
pub fn check_complete_set(present_assets: &[String], version: &str) -> Result<(), String> {
    let missing = missing_targets(present_assets, version);
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "incomplete OS set for release {version}: missing {}",
            missing.join(", ")
        ))
    }
}

/// Is a git tag a release tag? Accepts `v0.1.0`, `v1.2.3`, `1.2.3`, and
/// `v0.1.0-rc.1` style tags (optional leading `v`, then `N.N.N` with an
/// optional `-` suffix). Everything else is not a release tag.
pub fn is_release_tag(tag: &str) -> bool {
    let t = tag.trim();
    let s = t.strip_prefix('v').unwrap_or(t);
    if s.is_empty() {
        return false;
    }
    // Split off any `-prerelease` suffix.
    let core = s.split('-').next().unwrap_or("");
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() != 3 {
        return false;
    }
    parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// Version string for asset names from a release tag: `v0.1.0` → `0.1.0`.
/// Returns `None` when the tag is not a release tag.
pub fn version_from_tag(tag: &str) -> Option<String> {
    if !is_release_tag(tag) {
        return None;
    }
    let t = tag.trim();
    Some(t.strip_prefix('v').unwrap_or(t).to_string())
}

/// Split a bare version (`0.1.0`, `v1.2.3-rc.1`) into its numeric-ish
/// core segments and its prerelease suffix. A leading `v` is stripped;
/// everything after the first `-` is the prerelease.
fn split_version(v: &str) -> (Vec<String>, Option<String>) {
    let s = v.trim().strip_prefix('v').unwrap_or(v.trim());
    let (core, pre) = match s.split_once('-') {
        Some((c, p)) => (c, Some(p.to_string())),
        None => (s, None),
    };
    let segments = core
        .split('.')
        .map(|p| p.to_string())
        .collect::<Vec<_>>();
    (segments, pre)
}

/// Compare two release versions (`0.1.0` vs `v0.2.0`, prerelease-aware).
/// Core segments compare numerically where both are numeric and
/// lexically otherwise (missing segments read as `0`). A version
/// without a prerelease outranks one with it; two prereleases compare
/// lexically. Used by the update check to name a newer release
/// (Sprint 27 T002 / CAP-003).
pub fn compare_release_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let (a_seg, a_pre) = split_version(a);
    let (b_seg, b_pre) = split_version(b);
    let n = a_seg.len().max(b_seg.len());
    for i in 0..n {
        let x = a_seg.get(i).map(String::as_str).unwrap_or("0");
        let y = b_seg.get(i).map(String::as_str).unwrap_or("0");
        let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(xn), Ok(yn)) => xn.cmp(&yn),
            _ => x.cmp(y),
        };
        if ord != std::cmp::Ordering::Equal {
            return ord;
        }
    }
    match (&a_pre, &b_pre) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(_), None) => std::cmp::Ordering::Less,
        (Some(x), Some(y)) => x.cmp(y),
    }
}

/// Parse a `checksums.txt` / `checksums.sha256` file: one `<hex>  <name>`
/// per line (`sha256sum`-compatible; `#` comments and blank lines skipped).
/// Returns `(name, hex)` pairs with the hex lowercased.
pub fn parse_checksums_file(body: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, char::is_whitespace);
        let hex = parts.next().unwrap_or("").trim();
        let name = parts.next().unwrap_or("").trim().to_string();
        if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        if name.is_empty() {
            continue;
        }
        out.push((name, hex.to_lowercase()));
    }
    out
}

/// Verify data's SHA-256 against an expected hex digest. Returns `Err`
/// with expected/found spelled out on mismatch so the installer refuses
/// naming the mismatch (REC-002).
pub fn verify_sha256(data: &[u8], expected_hex: &str) -> Result<(), String> {
    let found = sha256_hex(data);
    if found.to_lowercase() == expected_hex.to_lowercase() {
        Ok(())
    } else {
        Err(format!(
            "checksum mismatch: expected {expected_hex} but found {found}"
        ))
    }
}

/// Verify an asset against a parsed checksums table. The asset must be
/// named with a matching SHA-256; an unknown asset or a mismatch is an
/// error naming the asset.
pub fn verify_asset(
    asset_name: &str,
    data: &[u8],
    checksums: &[(String, String)],
) -> Result<(), String> {
    let Some((_, expected)) = checksums.iter().find(|(n, _)| n == asset_name) else {
        return Err(format!("no checksum entry for {asset_name} in release metadata"));
    };
    if let Err(e) = verify_sha256(data, expected) {
        return Err(format!("{asset_name}: {e}"));
    }
    Ok(())
}

/// Minimal SHA-256 (FIPS 180-4) over bytes, std-only. Mirrors the `ffmpeg`
/// crate's `sha256` module so package checksums use the same digest the
/// FFmpeg bootstrap verifies downloads against.
pub fn sha256_hex(data: &[u8]) -> String {
    hex(&sha256(data))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(DIGITS[(b >> 4) as usize] as char);
        s.push(DIGITS[(b & 0xf) as usize] as char);
    }
    s
}

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for block in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, slot) in w.iter_mut().enumerate().take(16) {
            let o = i * 4;
            *slot = u32::from_be_bytes([block[o], block[o + 1], block[o + 2], block[o + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn required_targets_are_four() {
        assert_eq!(REQUIRED_TARGETS.len(), 4);
        for slug in ["macos-arm64", "macos-x64", "windows-x64", "linux-x64"] {
            assert!(
                REQUIRED_TARGETS.iter().any(|t| t.slug == slug),
                "missing slug {slug}"
            );
        }
    }

    #[test]
    fn asset_names() {
        for t in REQUIRED_TARGETS {
            let name = t.asset_name("0.1.0");
            assert!(name.starts_with("framesmith-0.1.0-"), "{name}");
            assert!(name.contains(t.slug), "{name}");
            if t.os == "windows" {
                assert!(name.ends_with(".zip"), "{name}");
            } else {
                assert!(name.ends_with(".tar.gz"), "{name}");
            }
        }
    }

    #[test]
    fn target_lookup_accepts_rust_spellings() {
        assert_eq!(target_for_os_arch("macos", "aarch64").unwrap().slug, "macos-arm64");
        assert_eq!(target_for_os_arch("macos", "arm64").unwrap().slug, "macos-arm64");
        assert_eq!(target_for_os_arch("macos", "x86_64").unwrap().slug, "macos-x64");
        assert_eq!(target_for_os_arch("macos", "x64").unwrap().slug, "macos-x64");
        assert_eq!(target_for_os_arch("windows", "x86_64").unwrap().slug, "windows-x64");
        assert_eq!(target_for_os_arch("linux", "x86_64").unwrap().slug, "linux-x64");
        assert!(target_for_os_arch("windows", "arm64").is_none());
        assert!(target_for_os_arch("linux", "aarch64").is_none());
        assert!(target_for_os_arch("freebsd", "x86_64").is_none());
    }

    #[test]
    fn incomplete_set_names_missing() {
        let v = "1.2.3";
        let all: Vec<String> = REQUIRED_TARGETS.iter().map(|t| t.asset_name(v)).collect();
        assert!(check_complete_set(&all, v).is_ok());
        // Drop macos-arm64 and linux-x64.
        let partial: Vec<String> = all
            .iter()
            .filter(|a| !a.contains("macos-arm64") && !a.contains("linux-x64"))
            .cloned()
            .collect();
        let err = check_complete_set(&partial, v).unwrap_err();
        assert!(err.contains("macos-arm64"), "{err}");
        assert!(err.contains("linux-x64"), "{err}");
        let missing = missing_targets(&partial, v);
        assert_eq!(missing.len(), 2);
    }

    #[test]
    fn release_tag_parsing() {
        for tag in ["v0.1.0", "v1.2.3", "1.2.3", "v0.1.0-rc.1"] {
            assert!(is_release_tag(tag), "{tag} should be a release tag");
        }
        for tag in ["", "v1.2", "main", "nightly", "v", "v1.2.3.4", "v1.2.x"] {
            assert!(!is_release_tag(tag), "{tag} should not be a release tag");
        }
        assert_eq!(version_from_tag("v1.2.3").as_deref(), Some("1.2.3"));
        assert_eq!(version_from_tag("v1.2.3-rc.1").as_deref(), Some("1.2.3-rc.1"));
        assert_eq!(version_from_tag("main"), None);
    }

    #[test]
    fn checksum_parse_and_verify() {
        let data = b"hello";
        let hex = sha256_hex(data);
        let table = format!("{hex}  framesmith-0.1.0-macos-arm64.tar.gz\n# comment\n");
        let parsed = parse_checksums_file(&table);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].0, "framesmith-0.1.0-macos-arm64.tar.gz");
        assert_eq!(parsed[0].1, hex);
        assert!(verify_asset("framesmith-0.1.0-macos-arm64.tar.gz", data, &parsed).is_ok());
        // Tamper refusal.
        let err = verify_asset("framesmith-0.1.0-macos-arm64.tar.gz", b"tampered", &parsed).unwrap_err();
        assert!(err.contains("checksum mismatch"), "{err}");
        // Unknown asset.
        let err = verify_asset("unknown.tar.gz", data, &parsed).unwrap_err();
        assert!(err.contains("no checksum entry"), "{err}");
        // Case-insensitive exact.
        assert!(verify_sha256(data, &hex.to_uppercase()).is_ok());
        assert!(verify_sha256(b"tamper", &hex).is_err());
    }

    #[test]
    fn release_version_ordering() {
        use std::cmp::Ordering::*;
        assert_eq!(compare_release_versions("0.1.0", "0.2.0"), Less);
        assert_eq!(compare_release_versions("0.2.0", "0.1.0"), Greater);
        assert_eq!(compare_release_versions("v0.1.0", "0.1.0"), Equal);
        assert_eq!(compare_release_versions("1.2.3", "1.2.3"), Equal);
        assert_eq!(compare_release_versions("1.10.0", "1.9.0"), Greater);
        assert_eq!(compare_release_versions("1.0.0-rc.1", "1.0.0"), Less);
        assert_eq!(compare_release_versions("1.0.0", "1.0.0-rc.1"), Greater);
        assert_eq!(
            compare_release_versions("1.0.0-rc.1", "1.0.0-rc.2"),
            Less
        );
        assert_eq!(compare_release_versions("2.0.0", "1.9.9"), Greater);
    }

    #[test]
    fn windows_binary_and_installer() {
        assert_eq!(binary_name_for("windows"), "framesmith.exe");
        assert_eq!(binary_name_for("macos"), "framesmith");
        assert_eq!(binary_name_for("linux"), "framesmith");
        assert_eq!(installer_for("windows"), "install.ps1");
        assert_eq!(installer_for("macos"), "install.sh");
    }

    #[test]
    fn supported_line_names_all() {
        let line = supported_platforms_line();
        for slug in ["macos-arm64", "macos-x64", "windows-x64", "linux-x64"] {
            assert!(line.contains(slug), "{line}");
        }
    }
}
