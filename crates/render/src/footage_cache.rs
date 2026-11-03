//! Project-local content-keyed footage frame cache (Sprint 5 T006).
//!
//! Rendering a footage project twice with unchanged media bytes and
//! unchanged footage attributes reuses already decoded clip windows:
//! the second render skips the FFmpeg decode subprocess and is faster
//! while staying byte-identical. Editing any media byte changes the
//! content hash, so edited files always decode fresh — a stale picture
//! can never be served.
//!
//! * Key: FNV-1a over the media file bytes plus the exact decode inputs
//!   (`seek`, `src_len`, oriented dimensions, container rotation, the
//!   FFmpeg binary identity, and ffprobe availability, which selects the
//!   decode start/budget path). The on-disk header re-checks every key
//!   field, so even a filename collision degrades to a miss, never to a
//!   wrong picture.
//! * Location: `<project>/logs/framesmith-footage-cache/` (project-local;
//!   cross-project sharing is deferred). The top-level project layout is
//!   unchanged, so `check` purity guarantees still hold.
//! * Bounded: total entry bytes stay under `FRAMESMITH_CACHE_MAX_BYTES`
//!   (default 1 GiB) with oldest-first eviction. Eviction only costs
//!   speed: a missing entry is just a miss followed by a fresh decode.
//! * Safe bypass (REC-004): any unreadable, truncated, or corrupt entry
//!   is deleted best-effort and counted as a miss; the render stays
//!   correct. A missing or unwritable cache directory disables caching
//!   for the run (all misses) without failing the render.
//! * Worker independence: entries store verbatim decode bytes, and every
//!   worker draws from the same [`DecodedRange`]s, so any
//!   `FRAMESMITH_WORKERS` value renders the same MP4.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::render::DecodedRange;

/// Hit/miss totals for one render: one lookup per decoded footage clip
/// window (never-visible windows decode nothing and count nothing).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
}

/// Directory holding cache entries inside the project log dir.
const CACHE_DIR_NAME: &[&str] = &["logs", "framesmith-footage-cache"];
/// Entry filename prefix and suffix (`fc-<key-hex>.bin`).
const ENTRY_PREFIX: &str = "fc-";
const ENTRY_SUFFIX: &str = ".bin";
/// Temporary store suffix, removed best-effort on open.
const TMP_SUFFIX: &str = ".tmp";
/// Default bound on total entry bytes (1 GiB).
const DEFAULT_MAX_BYTES: u64 = 1024 * 1024 * 1024;
/// Environment override for the byte bound (parsed as `u64`; anything
/// else keeps the default). Tests shrink it to force eviction.
const MAX_BYTES_ENV: &str = "FRAMESMITH_CACHE_MAX_BYTES";
/// On-disk magic (8 bytes).
const MAGIC: &[u8; 8] = b"FSFCv1\0\0";

/// Streaming FNV-1a (64-bit, no external hash dependency) shared by
/// file hashing, key derivation, and decoder identity.
struct Fnv64(u64);

impl Fnv64 {
    fn new() -> Self {
        Self(14695981039346656037)
    }

    fn update(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(1099511628211);
        }
    }

    fn update_u64(&mut self, v: u64) {
        self.update(&v.to_le_bytes());
    }

    fn finish(self) -> u64 {
        self.0
    }
}

/// FNV-1a over bytes (test-only shorthand; production paths stream
/// through [`Fnv64`]).
#[cfg(test)]
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h = Fnv64::new();
    h.update(bytes);
    h.finish()
}

/// Hash one media file's bytes without loading it whole.
fn hash_file(path: &Path) -> std::io::Result<u64> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Fnv64::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finish())
}

/// Identity of the decoder binary behind one render: its size plus
/// modification time, so a swapped FFmpeg never reads another
/// binary's pictures (it just misses and decodes fresh).
fn ffmpeg_id(ffmpeg_bin: &Path) -> u64 {
    let meta = std::fs::metadata(ffmpeg_bin);
    let (len, secs, nanos) = match meta.and_then(|m| m.modified().map(|t| (m.len(), t))) {
        Ok((len, modified)) => match modified.duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => (len, d.as_secs(), d.subsec_nanos() as u64),
            Err(_) => (len, 0, 0),
        },
        Err(_) => (0, 0, 0),
    };
    let mut h = Fnv64::new();
    for v in [len, secs, nanos] {
        h.update_u64(v);
    }
    h.update(ffmpeg_bin.as_os_str().as_encoded_bytes());
    h.finish()
}

/// Byte bound for total entry storage.
fn max_bytes() -> u64 {
    std::env::var(MAX_BYTES_ENV)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_MAX_BYTES)
}

/// Everything one cached decode is keyed on.
#[derive(Debug, Clone)]
struct EntryKey {
    file_name: String,
    content_hash: u64,
    seek_bits: u64,
    srclen_bits: u64,
    width: u32,
    height: u32,
    rotation_bits: u64,
    ffmpeg: u64,
    ffprobe: bool,
}

fn entry_key(
    content_hash: u64,
    seek: f64,
    src_len: f64,
    width: u32,
    height: u32,
    rotation: f64,
    ffmpeg: u64,
    ffprobe: bool,
) -> EntryKey {
    // The filename is only a hint: the header re-checks every field, so
    // a hash collision still ends in a miss, never a wrong picture.
    let mut h = Fnv64::new();
    for v in [
        content_hash,
        seek.to_bits(),
        src_len.to_bits(),
        rotation.to_bits(),
        ffmpeg,
    ] {
        h.update_u64(v);
    }
    for v in [width as u64, height as u64, u64::from(ffprobe)] {
        h.update_u64(v);
    }
    let h = h.finish();
    EntryKey {
        file_name: format!("{ENTRY_PREFIX}{h:016x}{ENTRY_SUFFIX}"),
        content_hash,
        seek_bits: seek.to_bits(),
        srclen_bits: src_len.to_bits(),
        width,
        height,
        rotation_bits: rotation.to_bits(),
        ffmpeg,
        ffprobe,
    }
}

/// Serialize one decoded window: header plus verbatim pictures and
/// presentation timestamps.
fn encode_entry(key: &EntryKey, range: &DecodedRange) -> Vec<u8> {
    let mut out = Vec::with_capacity(64 + range.frames.len() + range.pts.len() * 8);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&key.content_hash.to_le_bytes());
    out.extend_from_slice(&key.seek_bits.to_le_bytes());
    out.extend_from_slice(&key.srclen_bits.to_le_bytes());
    out.extend_from_slice(&key.width.to_le_bytes());
    out.extend_from_slice(&key.height.to_le_bytes());
    out.extend_from_slice(&key.rotation_bits.to_le_bytes());
    out.extend_from_slice(&key.ffmpeg.to_le_bytes());
    out.push(u8::from(key.ffprobe));
    out.extend_from_slice(&(range.pts.len() as u64).to_le_bytes());
    out.extend_from_slice(&range.frames);
    for &t in &range.pts {
        out.extend_from_slice(&t.to_le_bytes());
    }
    out
}

/// Read `n` little-endian bytes at `*off`, advancing it. `None` on
/// overrun (the entry is truncated).
fn read_u64(bytes: &[u8], off: &mut usize, n: usize) -> Option<u64> {
    let end = off.checked_add(n)?;
    let chunk = bytes.get(*off..end)?;
    *off = end;
    let mut v: u64 = 0;
    for (i, &b) in chunk.iter().enumerate() {
        v |= (b as u64) << (8 * i);
    }
    Some(v)
}

/// Parse and fully validate one entry file: magic, every key field, and
/// exact byte layout. `None` means corrupt — the caller deletes the
/// file and counts a miss.
fn decode_entry(bytes: &[u8], key: &EntryKey, video_idx: usize) -> Option<DecodedRange> {
    const HDR: usize = 8 + 8 * 5 + 4 * 2 + 8 + 1 + 8;
    if bytes.len() < HDR || &bytes[..8] != MAGIC {
        return None;
    }
    let mut off = 8;
    let content_hash = read_u64(bytes, &mut off, 8)?;
    let seek_bits = read_u64(bytes, &mut off, 8)?;
    let srclen_bits = read_u64(bytes, &mut off, 8)?;
    let width = read_u64(bytes, &mut off, 4)? as u32;
    let height = read_u64(bytes, &mut off, 4)? as u32;
    let rotation_bits = read_u64(bytes, &mut off, 8)?;
    let ffmpeg = read_u64(bytes, &mut off, 8)?;
    let ffprobe_byte = *bytes.get(off)?;
    off += 1;
    let count = read_u64(bytes, &mut off, 8)? as usize;
    if content_hash != key.content_hash
        || seek_bits != key.seek_bits
        || srclen_bits != key.srclen_bits
        || width != key.width
        || height != key.height
        || rotation_bits != key.rotation_bits
        || ffmpeg != key.ffmpeg
        || ffprobe_byte != u8::from(key.ffprobe)
    {
        return None;
    }
    let framesize = width as usize * height as usize * 3;
    if framesize == 0 || count == 0 {
        return None;
    }
    let (frames_len, pts_len) = (framesize.checked_mul(count)?, count.checked_mul(8)?);
    if bytes.len() != off + frames_len + pts_len {
        return None;
    }
    let frames = bytes[off..off + frames_len].to_vec();
    off += frames_len;
    let mut pts = Vec::with_capacity(count);
    for k in 0..count {
        pts.push(f64::from_le_bytes(bytes[off + k * 8..off + k * 8 + 8].try_into().ok()?));
        if !pts[k].is_finite() {
            return None;
        }
    }
    Some(DecodedRange {
        video_idx,
        width,
        height,
        frames,
        pts,
    })
}

/// Project-local footage frame cache for one render. Every method is
/// best-effort file I/O: failures disable or bypass, never fail the
/// render.
pub struct FootageCache {
    dir: Option<PathBuf>,
    max_bytes: u64,
    stats: CacheStats,
    content_hashes: HashMap<PathBuf, Option<u64>>,
}

impl FootageCache {
    /// Open the project cache, creating the directory. Any failure
    /// disables caching for the run (all misses, correct decodes).
    pub fn open(folder: &Path) -> Self {
        let mut dir = folder.to_path_buf();
        for part in CACHE_DIR_NAME {
            dir.push(part);
        }
        let max_bytes = max_bytes();
        let enabled = std::fs::create_dir_all(&dir).is_ok();
        let this = Self {
            dir: enabled.then_some(dir),
            max_bytes,
            stats: CacheStats::default(),
            content_hashes: HashMap::new(),
        };
        if enabled {
            this.sweep_tmp();
            // Every render starts inside the bound — including when the
            // bound shrank since the entries were stored, or when prior
            // hits skipped the post-store enforcement below.
            if let Some(d) = &this.dir {
                this.enforce_bound(d);
            }
        }
        this
    }

    /// Hit/miss totals so far.
    pub fn stats(&self) -> CacheStats {
        self.stats
    }

    /// Decode one clip window through the cache: content-keyed hit
    /// returns the stored pictures verbatim; anything else decodes
    /// fresh via `decode`, stores best-effort, and returns the fresh
    /// pictures. The returned range always matches the current media
    /// bytes — never a stale picture.
    pub(crate) fn decode_window(
        &mut self,
        ffmpeg_bin: &Path,
        abs_path: &Path,
        seek: f64,
        src_len: f64,
        width: u32,
        height: u32,
        rotation: f64,
        video_idx: usize,
        decode: impl FnOnce() -> Result<DecodedRange, String>,
    ) -> Result<DecodedRange, String> {
        let dir = match &self.dir {
            Some(d) => d.clone(),
            None => {
                self.stats.misses += 1;
                return decode();
            }
        };
        let content_hash = match self.content_hash(abs_path) {
            Some(h) => h,
            None => {
                // Unreadable now: the decode below reports the real
                // refusal naming the file; the cache just stays out.
                self.stats.misses += 1;
                return decode();
            }
        };
        let ffprobe = crate::render::ffprobe_for(ffmpeg_bin).is_some();
        let key = entry_key(
            content_hash,
            seek,
            src_len,
            width,
            height,
            rotation,
            ffmpeg_id(ffmpeg_bin),
            ffprobe,
        );
        let path = dir.join(&key.file_name);
        if let Some(range) = self.load(&path, &key, video_idx) {
            self.stats.hits += 1;
            return Ok(range);
        }
        self.stats.misses += 1;
        let mut range = decode()?;
        range.video_idx = video_idx;
        self.store(&dir, &path, &key, &range);
        Ok(range)
    }

    /// Memoized content hash for one absolute media path (`None` when
    /// the file cannot be read right now). The memo lives on the
    /// per-render cache instance, so every render re-hashes: an edit
    /// between renders always misses, never reads stale.
    fn content_hash(&mut self, abs: &Path) -> Option<u64> {
        if let Some(hit) = self.content_hashes.get(abs) {
            return *hit;
        }
        let hash = hash_file(abs).ok();
        self.content_hashes.insert(abs.to_path_buf(), hash);
        hash
    }

    /// Load and fully validate one entry. Corrupt entries are deleted
    /// best-effort (REC-004 bypass) and read as a miss.
    fn load(&self, path: &Path, key: &EntryKey, video_idx: usize) -> Option<DecodedRange> {
        let bytes = std::fs::read(path).ok()?;
        match decode_entry(&bytes, key, video_idx) {
            Some(range) => Some(range),
            None => {
                let _ = std::fs::remove_file(path);
                None
            }
        }
    }

    /// Store one fresh decode, then evict oldest-first back under the
    /// byte bound. Entries larger than the whole bound are never stored:
    /// the bound holds strictly, and skipping only costs speed.
    fn store(&self, dir: &Path, path: &Path, key: &EntryKey, range: &DecodedRange) {
        if self.max_bytes == 0 {
            return;
        }
        let bytes = encode_entry(key, range);
        if bytes.len() as u64 > self.max_bytes {
            return;
        }
        let tmp = dir.join(format!(
            "{}{TMP_SUFFIX}-{}",
            key.file_name,
            std::process::id()
        ));
        if std::fs::write(&tmp, &bytes).is_err() {
            let _ = std::fs::remove_file(&tmp);
            return;
        }
        if std::fs::rename(&tmp, path).is_err() {
            let _ = std::fs::remove_file(&tmp);
            return;
        }
        self.enforce_bound(dir);
    }

    /// Remove oldest-first (by mtime, name tiebreak for determinism)
    /// until total entry bytes fit the bound. Deletions only cost speed.
    fn enforce_bound(&self, dir: &Path) {
        let entries = match read_entries(dir) {
            Some(e) => e,
            None => return,
        };
        let total: u64 = entries.iter().map(|(_, s, _)| *s).sum();
        if total <= self.max_bytes {
            return;
        }
        let mut sorted = entries;
        sorted.sort_by(|a, b| a.2.cmp(&b.2).then_with(|| a.0.cmp(&b.0)));
        let mut running = total;
        for (name, size, _) in sorted {
            if running <= self.max_bytes {
                break;
            }
            if std::fs::remove_file(dir.join(&name)).is_ok() {
                running = running.saturating_sub(size);
            }
        }
    }

    /// Best-effort removal of leftover store temp files from crashed
    /// renders. Readers never see partial files (atomic rename), so
    /// this is hygiene only.
    fn sweep_tmp(&self) {
        let dir = match &self.dir {
            Some(d) => d,
            None => return,
        };
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.contains(TMP_SUFFIX) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

/// `(name, bytes, mtime)` of every cache entry file in `dir`.
fn read_entries(dir: &Path) -> Option<Vec<(String, u64, (u64, u64, u64))>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).ok()? .flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(ENTRY_PREFIX) || !name.ends_with(ENTRY_SUFFIX) {
            continue;
        }
        let meta = entry.metadata().ok()?;
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| (d.as_secs(), d.subsec_nanos() as u64, meta.len()))
            .unwrap_or((0, 0, meta.len()));
        out.push((name, meta.len(), mtime));
    }
    Some(out)
}

/// Test hook: the cache directory for one project folder.
#[cfg(test)]
pub(crate) fn cache_dir_for(folder: &Path) -> PathBuf {
    let mut dir = folder.to_path_buf();
    for part in CACHE_DIR_NAME {
        dir.push(part);
    }
    dir
}

/// Test hook: hash a short label with the same function the keys use.
#[cfg(test)]
pub(crate) fn hash_label(label: &str) -> u64 {
    fnv1a64(label.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_for(content: &[u8]) -> EntryKey {
        entry_key(fnv1a64(content), 1.0, 2.0, 64, 48, 0.0, 7, true)
    }

    fn range_of(video_idx: usize, n: usize) -> DecodedRange {
        let framesize = 64 * 48 * 3;
        let mut frames = vec![0u8; framesize * n];
        for (i, b) in frames.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        DecodedRange {
            video_idx,
            width: 64,
            height: 48,
            frames,
            pts: (0..n).map(|i| i as f64 / 30.0).collect(),
        }
    }

    #[test]
    fn fnv_is_deterministic_and_content_sensitive() {
        assert_eq!(fnv1a64(b"abc"), fnv1a64(b"abc"));
        assert_ne!(fnv1a64(b"abc"), fnv1a64(b"abd"));
        assert_ne!(hash_label("a"), hash_label("b"));
    }

    #[test]
    fn round_trip_preserves_pictures_and_timestamps() {
        let key = key_for(b"media");
        let range = range_of(3, 4);
        let bytes = encode_entry(&key, &range);
        let back = decode_entry(&bytes, &key, 3).expect("valid entry parses");
        assert_eq!(back.video_idx, 3);
        assert_eq!((back.width, back.height), (64, 48));
        assert_eq!(back.frames, range.frames);
        assert_eq!(back.pts, range.pts);
    }

    #[test]
    fn any_key_field_change_is_a_miss_not_a_wrong_picture() {
        let range = range_of(0, 2);
        let base = key_for(b"media");
        let bytes = encode_entry(&base, &range);
        // One flipped media byte, moved window, resized picture, rotated
        // source, swapped decoder, changed ffprobe path: every variant
        // must fail validation rather than serve the stored pictures.
        let variants = [
            key_for(b"medib"),
            entry_key(base.content_hash, 1.5, 2.0, 64, 48, 0.0, 7, true),
            entry_key(base.content_hash, 1.0, 2.5, 64, 48, 0.0, 7, true),
            entry_key(base.content_hash, 1.0, 2.0, 128, 48, 0.0, 7, true),
            entry_key(base.content_hash, 1.0, 2.0, 64, 48, 90.0, 7, true),
            entry_key(base.content_hash, 1.0, 2.0, 64, 48, 0.0, 8, true),
            entry_key(base.content_hash, 1.0, 2.0, 64, 48, 0.0, 7, false),
        ];
        for v in variants {
            assert!(decode_entry(&bytes, &v, 0).is_none(), "variant {v:?} must miss");
        }
    }

    #[test]
    fn corrupt_and_truncated_entries_are_misses() {
        let key = key_for(b"media");
        let bytes = encode_entry(&key, &range_of(0, 3));
        // Wrong magic.
        let mut bad = bytes.clone();
        bad[0] ^= 0xFF;
        assert!(decode_entry(&bad, &key, 0).is_none());
        // Truncated pictures, truncated timestamps, trailing garbage.
        assert!(decode_entry(&bytes[..bytes.len() - 1], &key, 0).is_none());
        assert!(decode_entry(&bytes[..bytes.len() / 2], &key, 0).is_none());
        let mut long = bytes.clone();
        long.push(0);
        assert!(decode_entry(&long, &key, 0).is_none());
        // Empty and tiny inputs.
        assert!(decode_entry(&[], &key, 0).is_none());
        assert!(decode_entry(&[1, 2, 3], &key, 0).is_none());
        // Non-finite timestamp.
        let mut range = range_of(0, 2);
        range.pts[1] = f64::NAN;
        let nan_bytes = encode_entry(&key, &range);
        assert!(decode_entry(&nan_bytes, &key, 0).is_none());
    }

    #[test]
    fn store_and_load_through_the_filesystem_with_bypass() {
        let folder = std::env::temp_dir().join(format!(
            "framesmith-footage-cache-unit-{}-{}",
            std::process::id(),
            fnv1a64(b"store-load") % 100000
        ));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let media = folder.join("clip.bin");
        std::fs::write(&media, b"fake-media-bytes").unwrap();
        let mut cache = FootageCache::open(&folder);
        assert!(cache.dir.is_some());
        let range = range_of(0, 2);
        let hit = cache
            .decode_window(
                Path::new("ffmpeg"),
                &media,
                1.0,
                2.0,
                64,
                48,
                0.0,
                0,
                || Ok(range.clone()),
            )
            .unwrap();
        assert_eq!(hit.frames, range.frames);
        assert_eq!(cache.stats(), CacheStats { hits: 0, misses: 1 });
        // Second lookup hits without calling the decoder (which would
        // fail the render if called).
        let hit2 = cache
            .decode_window(Path::new("ffmpeg"), &media, 1.0, 2.0, 64, 48, 0.0, 0, || {
                Err("decoder must not run on a hit".to_string())
            })
            .unwrap();
        assert_eq!(hit2.frames, range.frames);
        assert_eq!(hit2.pts, range.pts);
        assert_eq!(cache.stats(), CacheStats { hits: 1, misses: 1 });
        // Corrupt the entry on disk: the next lookup bypasses it (miss),
        // decodes fresh, and repairs the file.
        let dir = cache_dir_for(&folder);
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .collect();
        assert_eq!(names.len(), 1);
        std::fs::write(&names[0], b"garbage-not-an-entry").unwrap();
        let fresh = range_of(0, 2);
        let hit3 = cache
            .decode_window(Path::new("ffmpeg"), &media, 1.0, 2.0, 64, 48, 0.0, 0, || {
                Ok(fresh.clone())
            })
            .unwrap();
        assert_eq!(hit3.frames, fresh.frames);
        assert_eq!(cache.stats(), CacheStats { hits: 1, misses: 2 });
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn edited_media_bytes_never_read_stale() {
        let folder = std::env::temp_dir().join(format!(
            "framesmith-footage-cache-fresh-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let media = folder.join("clip.bin");
        std::fs::write(&media, b"version-one").unwrap();
        // First render: miss, decode, store.
        let mut first_render = FootageCache::open(&folder);
        let first = range_of(0, 1);
        first_render
            .decode_window(Path::new("ffmpeg"), &media, 0.0, 1.0, 64, 48, 0.0, 0, || {
                Ok(first.clone())
            })
            .unwrap();
        assert_eq!(first_render.stats().misses, 1);
        // One flipped byte, then a second render with a fresh cache
        // instance (as `prepare_frames` opens per render): the old entry
        // must not serve.
        std::fs::write(&media, b"version-two").unwrap();
        let second = {
            let mut r = range_of(0, 1);
            r.frames[0] = 0xAB;
            r
        };
        let mut second_render = FootageCache::open(&folder);
        let mut decoder_ran = false;
        let got = second_render
            .decode_window(Path::new("ffmpeg"), &media, 0.0, 1.0, 64, 48, 0.0, 0, || {
                decoder_ran = true;
                Ok(second.clone())
            })
            .unwrap();
        assert!(decoder_ran, "edited bytes must force a fresh decode");
        assert_eq!(got.frames, second.frames);
        assert_eq!(second_render.stats(), CacheStats { hits: 0, misses: 1 });
        let _ = std::fs::remove_dir_all(&folder);
    }
}
