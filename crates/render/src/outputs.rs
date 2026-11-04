//! Output destinations (Sprint 12 outputs, T001): the seven `--format`
//! values one composition can deliver, plus the `--prores-profile`
//! choice for ProRes MOVs.
//!
//! The default path (`--format mp4`, or no `--format` flag at all)
//! builds exactly the historical H.264 MP4 argv, so default renders
//! stay byte-identical. Every other format reuses the same drawn
//! frames, the same audio filtergraph (formats that carry audio), and
//! the same staged-temp-then-rename discipline (no partial output on
//! failure).

use std::path::{Path, PathBuf};

/// One `--format` destination (Sprint 12 outputs, CAP-001).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    /// H.264 MP4 (the historical default, unchanged).
    #[default]
    Mp4,
    /// HEVC (H.265) MP4.
    Hevc,
    /// VP9 WebM (keeps alpha when the composition has any).
    Webm,
    /// ProRes MOV (`--prores-profile` picks the profile).
    Prores,
    /// Looping GIF (palette-fixed, deterministic).
    Gif,
    /// PNG image sequence (`frame-%04d.png` plus the frame count).
    PngSeq,
    /// HLS (`index.m3u8` playlist plus segments).
    Hls,
}

impl OutputFormat {
    /// Exactly the seven accepted spellings; anything else is refused
    /// before the first frame with no output written.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "mp4" => Some(OutputFormat::Mp4),
            "hevc" => Some(OutputFormat::Hevc),
            "webm" => Some(OutputFormat::Webm),
            "prores" => Some(OutputFormat::Prores),
            "gif" => Some(OutputFormat::Gif),
            "pngseq" => Some(OutputFormat::PngSeq),
            "hls" => Some(OutputFormat::Hls),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            OutputFormat::Mp4 => "mp4",
            OutputFormat::Hevc => "hevc",
            OutputFormat::Webm => "webm",
            OutputFormat::Prores => "prores",
            OutputFormat::Gif => "gif",
            OutputFormat::PngSeq => "pngseq",
            OutputFormat::Hls => "hls",
        }
    }

    /// The accepted-values wording every refusal names.
    pub fn accepted() -> &'static str {
        "mp4, hevc, webm, prores, gif, pngseq, or hls"
    }

    /// Default output file name (or directory name for sequences) when
    /// no `-o` path is passed, resolved inside the project folder.
    pub fn default_file_name(self) -> &'static str {
        match self {
            OutputFormat::Mp4 => "out.mp4",
            OutputFormat::Hevc => "out.mp4",
            OutputFormat::Webm => "out.webm",
            OutputFormat::Prores => "out.mov",
            OutputFormat::Gif => "out.gif",
            OutputFormat::PngSeq => "frames",
            OutputFormat::Hls => "hls",
        }
    }

    /// File extension appended by batch naming (sequences resolve
    /// through their own layout, so this is only the file formats).
    pub fn default_extension(self) -> &'static str {
        match self {
            OutputFormat::Mp4 => "mp4",
            OutputFormat::Hevc => "mp4",
            OutputFormat::Webm => "webm",
            OutputFormat::Prores => "mov",
            OutputFormat::Gif => "gif",
            OutputFormat::PngSeq => "png",
            OutputFormat::Hls => "m3u8",
        }
    }

    /// True for destinations that carry the composition's audio (GIF
    /// loops silent pictures; PNG sequences are pictures only).
    pub fn carries_audio(self) -> bool {
        !matches!(self, OutputFormat::Gif | OutputFormat::PngSeq)
    }

    /// True for directory-style destinations (a PNG directory, an HLS
    /// directory holding the playlist plus segments).
    pub fn is_sequence(self) -> bool {
        matches!(self, OutputFormat::PngSeq | OutputFormat::Hls)
    }

    /// External video encoder the host FFmpeg must provide beyond the
    /// Sprint 1 pair (`libx264` plus `aac`); `None` means the format
    /// encodes with encoders every suitable FFmpeg ships (including
    /// the HLS path, which reuses the MP4 pair).
    pub fn required_video_encoder(self) -> Option<&'static str> {
        match self {
            OutputFormat::Hevc => Some("libx265"),
            OutputFormat::Webm => Some("libvpx-vp9"),
            OutputFormat::Prores => Some("prores_ks"),
            _ => None,
        }
    }

    /// External audio encoder the host FFmpeg must provide beyond the
    /// Sprint 1 pair; `None` means every suitable FFmpeg ships the
    /// destination's audio encoder (including silent destinations,
    /// which carry no audio at all).
    pub fn required_audio_encoder(self) -> Option<&'static str> {
        match self {
            OutputFormat::Webm => Some("libopus"),
            _ => None,
        }
    }

    /// Container format flag passed to FFmpeg (`-f`).
    pub fn container(self) -> &'static str {
        match self {
            OutputFormat::Mp4 | OutputFormat::Hevc => "mp4",
            OutputFormat::Webm => "webm",
            OutputFormat::Prores => "mov",
            OutputFormat::Gif => "gif",
            OutputFormat::PngSeq => "image2",
            OutputFormat::Hls => "hls",
        }
    }
}

/// One `--encoder` choice (Sprint 12 outputs, CAP-003): `auto`
/// uses the format's hardware encoder when the host FFmpeg provides
/// it and software otherwise without failing; `software` always
/// encodes on the CPU (deterministic: same inputs re-render
/// byte-identical); `videotoolbox` demands hardware encoding and
/// refuses before the first frame when the host lacks it. Absent
/// means `software`, so default renders stay byte-identical on every
/// machine (hardware encodes verify by decode, never by hash).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EncoderChoice {
    Auto,
    /// CPU encoding (the default when no `--encoder` flag is passed).
    #[default]
    Software,
    Videotoolbox,
}

impl EncoderChoice {
    /// Exactly the three accepted spellings; anything else is refused
    /// before the first frame with no output written.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "auto" => Some(EncoderChoice::Auto),
            "software" => Some(EncoderChoice::Software),
            "videotoolbox" => Some(EncoderChoice::Videotoolbox),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            EncoderChoice::Auto => "auto",
            EncoderChoice::Software => "software",
            EncoderChoice::Videotoolbox => "videotoolbox",
        }
    }

    /// The accepted-values wording every refusal names.
    pub fn accepted() -> &'static str {
        "auto, software, or videotoolbox"
    }
}

/// Parse one `--keyint` value (Sprint 12 outputs, CAP-003): a positive
/// whole number of frames (`Some`) or a refusal (`None`, naming the
/// accepted shape at the call site).
pub fn parse_keyint(raw: &str) -> Option<u32> {
    match raw.trim().parse::<u32>() {
        Ok(n) if n >= 1 => Some(n),
        _ => None,
    }
}

/// The accepted-values wording every `--keyint` refusal names.
pub fn keyint_accepted() -> &'static str {
    "a positive whole number of frames (1 or more)"
}

/// Parse one `--maxrate` value (Sprint 12 outputs, CAP-003): a positive
/// bitrate with an optional `k`/`K`/`m`/`M` suffix (for example `8M`
/// or `800k`), mirroring the `--video-bitrate` shape. Returns the
/// trimmed value on success, `None` on a malformed value (the call
/// site refuses naming the accepted shape).
pub fn parse_maxrate(raw: &str) -> Option<String> {
    let t = raw.trim();
    let num = match t.strip_suffix(['k', 'K', 'm', 'M']) {
        Some(n) => n,
        None => t,
    };
    match num.parse::<f64>() {
        Ok(x) if x.is_finite() && x > 0.0 => Some(t.to_string()),
        _ => None,
    }
}

/// The accepted-values wording every `--maxrate` refusal names.
pub fn maxrate_accepted() -> &'static str {
    "a positive bitrate with an optional k/K/m/M suffix (for example `8M` or `800k`)"
}

/// The `-bufsize` riding alongside one `--maxrate` value (Sprint 12
/// outputs, CAP-003): twice the cap in the same suffix, so the peak
/// has headroom for a single GOP while longer windows stay capped.
/// Only call with a value [`parse_maxrate`] accepted.
pub fn maxrate_bufsize(maxrate: &str) -> String {
    let t = maxrate.trim();
    let (num, suffix) = match t.strip_suffix(['k', 'K', 'm', 'M']) {
        Some(n) => (n, &t[t.len() - 1..]),
        None => (t, ""),
    };
    let doubled = num.parse::<f64>().unwrap_or(0.0) * 2.0;
    if doubled.fract() == 0.0 {
        format!("{}{suffix}", doubled as u64)
    } else {
        format!("{doubled}{suffix}")
    }
}

/// The host FFmpeg encoder `--encoder videotoolbox` (or `auto` on a
/// capable machine) writes for one destination (Sprint 12 outputs,
/// CAP-003): the VideoToolbox H.264/H.265 encoders for the
/// H.264/H.265 destinations. `None` means the destination has no
/// hardware encoder on this stack (WebM ships no VideoToolbox VP9
/// encoder; ProRes keeps its pinned `prores_ks` path; GIF and PNG
/// sequences carry no video stream to encode), so an explicit
/// `videotoolbox` there refuses while `auto` quietly encodes software.
pub fn hardware_encoder_for(format: OutputFormat) -> Option<&'static str> {
    match format {
        OutputFormat::Mp4 | OutputFormat::Hls => Some("h264_videotoolbox"),
        OutputFormat::Hevc => Some("hevc_videotoolbox"),
        OutputFormat::Webm | OutputFormat::Prores | OutputFormat::Gif | OutputFormat::PngSeq => {
            None
        }
    }
}

/// True when the host FFmpeg provides the destination's hardware
/// encoder (Sprint 12 outputs, CAP-003): the fact `auto` resolves on
/// and explicit `videotoolbox` demands. Setting
/// `FRAMESMITH_NO_VIDEOTOOLBOX=1` (or `true`) hides hardware even
/// when present, so the hardware-missing path exercises on machines
/// that ship VideoToolbox. Destinations without a hardware encoder
/// always report false.
pub fn hardware_available(ffmpeg_bin: &Path, format: OutputFormat) -> bool {
    let Some(want) = hardware_encoder_for(format) else {
        return false;
    };
    if std::env::var("FRAMESMITH_NO_VIDEOTOOLBOX")
        .map(|v| {
            let v = v.trim();
            v == "1" || v.eq_ignore_ascii_case("true")
        })
        .unwrap_or(false)
    {
        return false;
    }
    match encoders_text(ffmpeg_bin) {
        Ok(text) => text_has_encoder(&text, want),
        Err(_) => false,
    }
}

/// The host FFmpeg's `-encoders` listing (stdout plus stderr): the
/// shared probe behind the format and hardware checks.
fn encoders_text(ffmpeg_bin: &Path) -> Result<String, String> {
    let out = std::process::Command::new(ffmpeg_bin)
        .args(["-hide_banner", "-encoders"])
        .output()
        .map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr))
}

/// True when one `-encoders` listing line names `name` in its encoder
/// column (the second whitespace-separated field).
fn text_has_encoder(text: &str, name: &str) -> bool {
    text.lines().any(|line| {
        let mut parts = line.split_whitespace();
        parts.next();
        parts.next() == Some(name)
    })
}

/// One `--hdr` choice (Sprint 12 outputs, CAP-004): `auto` (the
/// default) renders HEVC Main10 BT.2020 when a source carries PQ/HLG
/// and SDR otherwise; `off` always renders SDR; `pq`/`hlg` demand
/// that transfer and refuse when no source carries it. Absent means
/// `auto`, so default renders stay byte-identical (SDR compositions
/// emit no HDR flags at all).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HdrMode {
    Off,
    #[default]
    Auto,
    Pq,
    Hlg,
}

impl HdrMode {
    /// Exactly the four accepted spellings; anything else is refused
    /// before the first frame with no output written.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "off" => Some(HdrMode::Off),
            "auto" => Some(HdrMode::Auto),
            "pq" => Some(HdrMode::Pq),
            "hlg" => Some(HdrMode::Hlg),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            HdrMode::Off => "off",
            HdrMode::Auto => "auto",
            HdrMode::Pq => "pq",
            HdrMode::Hlg => "hlg",
        }
    }

    /// The accepted-values wording every refusal names.
    pub fn accepted() -> &'static str {
        "off, pq, hlg, or auto"
    }
}

/// Effective transfer of one render (Sprint 12 outputs, CAP-004):
/// the decision the source probe drove. `Sdr` emits the historical
/// argv exactly; `Pq`/`Hlg` emit HEVC Main10 BT.2020 with that
/// transfer on the formats that can carry it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HdrTransfer {
    #[default]
    Sdr,
    Pq,
    Hlg,
}

impl HdrTransfer {
    pub fn name(self) -> &'static str {
        match self {
            HdrTransfer::Sdr => "sdr",
            HdrTransfer::Pq => "pq",
            HdrTransfer::Hlg => "hlg",
        }
    }

    /// The `ffmpeg -i` transfer token the probe looks for: PQ is
    /// SMPTE ST 2084, HLG is ARIB STD-B67. Mastering-display
    /// `smpte2086` never matches either (it names the display, not
    /// the transfer).
    pub fn probe_token(self) -> &'static str {
        match self {
            HdrTransfer::Sdr => "sdr",
            HdrTransfer::Pq => "smpte2084",
            HdrTransfer::Hlg => "arib-std-b67",
        }
    }

    /// The libx265 `transfer=` value writing this transfer's VUI, so
    /// the probed output carries the matching `color_transfer`.
    pub fn x265_transfer(self) -> &'static str {
        match self {
            HdrTransfer::Sdr => "",
            HdrTransfer::Pq => "smpte2084",
            HdrTransfer::Hlg => "arib-std-b67",
        }
    }
}

/// Detect one source's transfer from its `ffmpeg -i` text (Sprint 12
/// outputs, CAP-004): PQ when the text names SMPTE 2084, HLG when it
/// names ARIB STD-B67, SDR otherwise. PQ wins when a confused
/// container names both.
pub fn detect_transfer(text: &str) -> HdrTransfer {
    let lower = text.to_lowercase();
    if lower.contains("smpte2084")
        || lower.contains("smpte-st-2084")
        || lower.contains("smpte_st_2084")
        || lower.contains("st2084")
    {
        HdrTransfer::Pq
    } else if lower.contains("arib-std-b67") || lower.contains("arib_std_b67") {
        HdrTransfer::Hlg
    } else {
        HdrTransfer::Sdr
    }
}

/// True for destinations that can carry an HDR picture (Sprint 12
/// outputs, CAP-004): the MP4-family containers and HLS, which carry
/// HEVC Main10 BT.2020 when HDR is active (an MP4 render then carries
/// HEVC instead of its default H.264). WebM, ProRes, GIF, and PNG
/// sequences cannot — explicit HDR there refuses naming the format
/// limit, and `auto` with an HDR source refuses there too except for
/// the tone-mapped SDR-only destinations (GIF, PNG sequences).
pub fn format_carries_hdr(format: OutputFormat) -> bool {
    matches!(
        format,
        OutputFormat::Mp4 | OutputFormat::Hevc | OutputFormat::Hls
    )
}

/// True for destinations whose pictures are SDR by nature (Sprint 12
/// outputs, CAP-004): an HDR source renders there tone-mapped under
/// `auto` (the report notes the mapping) and refuses under an
/// explicit `--hdr pq`/`hlg`.
pub fn format_tone_maps_hdr(format: OutputFormat) -> bool {
    matches!(format, OutputFormat::Gif | OutputFormat::PngSeq)
}

/// The `-x265-params` writing one HDR transfer's BT.2020 Main10 VUI
/// (Sprint 12 outputs, CAP-004): primaries, transfer, and matrix name
/// BT.2020, so the probed output carries `color_primaries=bt2020`,
/// `color_transfer=smpte2084|arib-std-b67`, `color_space=bt2020nc` at
/// `yuv420p10le`. Only call with a non-SDR transfer.
pub fn x265_hdr_params(transfer: HdrTransfer) -> String {
    format!(
        "colorprim=bt2020:transfer={}:colormatrix=bt2020nc",
        transfer.x265_transfer()
    )
}

/// One `--prores-profile` choice (Sprint 12 outputs, CAP-001):
/// the five Apple ProRes profiles, defaulting to `standard`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProresProfile {
    Proxy,
    Lt,
    #[default]
    Standard,
    Hq,
    P4444,
}

impl ProresProfile {
    /// Exactly the five accepted spellings; anything else is refused
    /// before the first frame with no output written.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "proxy" => Some(ProresProfile::Proxy),
            "lt" => Some(ProresProfile::Lt),
            "standard" => Some(ProresProfile::Standard),
            "hq" => Some(ProresProfile::Hq),
            "4444" => Some(ProresProfile::P4444),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ProresProfile::Proxy => "proxy",
            ProresProfile::Lt => "lt",
            ProresProfile::Standard => "standard",
            ProresProfile::Hq => "hq",
            ProresProfile::P4444 => "4444",
        }
    }

    /// The accepted-values wording every refusal names.
    pub fn accepted() -> &'static str {
        "proxy, lt, standard, hq, or 4444"
    }

    /// The `prores_ks` profile number (`-profile:v`).
    pub fn ffmpeg_profile(self) -> &'static str {
        match self {
            ProresProfile::Proxy => "0",
            ProresProfile::Lt => "1",
            ProresProfile::Standard => "2",
            ProresProfile::Hq => "3",
            ProresProfile::P4444 => "4",
        }
    }

    /// Output pixel format for the profile: 10-bit 4:2:2 everywhere;
    /// `4444` encodes full-resolution 4:4:4, which `prores_ks` writes
    /// at 12-bit depth (opaque, like the canvas).
    pub fn pix_fmt(self) -> &'static str {
        match self {
            ProresProfile::P4444 => "yuv444p12le",
            _ => "yuv422p10le",
        }
    }
}

/// Provenance stamped on every delivered output (Sprint 12 outputs,
/// CAP-005): the tool name, the tool version, and a digest of the
/// composition inputs. The caller computes the digest from the
/// validated compositions plus their media bytes and passes it here;
/// this module only formats the stamp and names where it lands.
/// Metadata never touches pixels or samples, only container tags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    /// Tool version, exactly as `framesmith --version` reports it.
    pub version: String,
    /// Hex digest of the composition inputs for this render.
    pub digest: String,
}

impl Provenance {
    /// The stamping tool, shared by every destination.
    pub fn tool() -> &'static str {
        "framesmith"
    }

    /// The single stamp string every probe reads back: the tool name,
    /// the version, and the inputs digest.
    pub fn stamp(&self) -> String {
        format!("{} {} inputs={}", Self::tool(), self.version, self.digest)
    }

    /// The PNG-sequence sidecar body (Sprint 12 outputs, CAP-005): the
    /// same tool, version, and digest fields the containers carry as
    /// tags. The version and digest are caller-controlled version and
    /// hex strings (no quoting characters), so plain formatting is
    /// exact.
    pub fn sidecar_json(&self) -> String {
        format!(
            "{{\"tool\":\"{}\",\"version\":\"{}\",\"digest\":\"{}\"}}",
            Self::tool(),
            self.version,
            self.digest
        )
    }
}

/// Digest of the composition inputs for one render (Sprint 12 outputs,
/// CAP-005): FNV-1a 64 over the canonical composition encoding plus
/// the bytes of every referenced media file. `files` holds
/// project-relative path strings with their bytes and is sorted here,
/// so call order never changes the digest. The same inputs re-render
/// the same hex; any input change re-stamps.
pub fn provenance_digest(canonical: &str, files: &[(String, Vec<u8>)]) -> String {
    let mut sorted: Vec<&(String, Vec<u8>)> = files.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut h: u64 = 0xcbf29ce484222325;
    let mut feed = |bytes: &[u8]| {
        for b in bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    };
    feed(canonical.as_bytes());
    feed(&[0xff]);
    for (name, bytes) in sorted {
        feed(name.as_bytes());
        feed(&[0xff]);
        feed(bytes);
        feed(&[0xff]);
    }
    format!("{h:016x}")
}

/// True for destinations that embed the stamp in the container
/// (Sprint 12 outputs, CAP-005): `-metadata comment=` on the encode
/// argv, readable through the standard probe (`ffprobe -show_entries
/// format_tags=comment`). PNG sequences carry a sibling sidecar JSON
/// file instead, HLS carries a playlist comment, and GIF carries
/// neither (the render warns naming the format without blocking
/// pixels).
pub fn format_embeds_provenance(format: OutputFormat) -> bool {
    matches!(
        format,
        OutputFormat::Mp4 | OutputFormat::Hevc | OutputFormat::Webm | OutputFormat::Prores
    )
}

/// The `-metadata` argv embedding one stamp in a container (Sprint 12
/// outputs, CAP-005): a single `comment` tag holding the tool name,
/// the version, and the inputs digest. Only call for formats where
/// [`format_embeds_provenance`] holds.
pub fn provenance_metadata_args(stamp: &str) -> Vec<String> {
    vec!["-metadata".to_string(), format!("comment={stamp}")]
}

/// The HLS playlist comment carrying one stamp (Sprint 12 outputs,
/// CAP-005): a `#` comment line, so segment verification (which skips
/// `#` lines) and players both pass over it while the probe reads it
/// back verbatim.
pub fn provenance_playlist_comment(stamp: &str) -> String {
    format!("# {stamp}")
}

/// Path of the PNG-sequence provenance sidecar (Sprint 12 outputs,
/// CAP-005): a sibling JSON file beside the sequence directory (the
/// frames directory itself keeps exactly `frame-%04d.png`), carrying
/// the same tool, version, and digest fields the containers carry as
/// tags.
pub fn provenance_sidecar_for_seq(dir: &Path) -> PathBuf {
    let mut name = dir.as_os_str().to_owned();
    name.push(".provenance.json");
    PathBuf::from(name)
}

/// One resolved render destination: a single file, a PNG directory,
/// or an HLS directory holding the playlist plus its segments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedOutput {
    /// A single output file (mp4, hevc, webm, prores, gif).
    File { path: PathBuf },
    /// A directory holding `frame-%04d.png`.
    PngSeq { dir: PathBuf },
    /// A directory holding the playlist plus segments.
    Hls { dir: PathBuf, playlist: PathBuf },
}

impl ResolvedOutput {
    /// The path the success line and the `--json` report name: the
    /// file, the PNG directory, or the HLS playlist.
    pub fn display_path(&self) -> &Path {
        match self {
            ResolvedOutput::File { path } => path,
            ResolvedOutput::PngSeq { dir } => dir,
            ResolvedOutput::Hls { playlist, .. } => playlist,
        }
    }

    /// True for directory-style destinations.
    pub fn is_sequence(&self) -> bool {
        !matches!(self, ResolvedOutput::File { .. })
    }
}

/// Resolve the render destination from the `--format` value and the
/// raw `-o` path (Sprint 12 outputs, T001): files stay files, `pngseq`
/// resolves to a directory, `hls` resolves to a directory holding
/// `index.m3u8` (or to the named `.m3u8` playlist with segments
/// beside it). Nothing is created here; the preflight in the caller
/// refuses unwritable paths before the first frame.
pub fn resolve_output(
    format: OutputFormat,
    folder: &Path,
    user_out: Option<&Path>,
) -> ResolvedOutput {
    // A caller `-o` path is used as given (matching the historical
    // file behavior: relative to the caller's working directory);
    // only the defaults resolve inside the project folder. Nothing is
    // created here; the caller preflights writability before frame one.
    match format {
        OutputFormat::PngSeq => {
            let dir = match user_out {
                Some(p) => PathBuf::from(p),
                None => folder.join(format.default_file_name()),
            };
            ResolvedOutput::PngSeq { dir }
        }
        OutputFormat::Hls => {
            let (dir, playlist) = match user_out {
                Some(p) => {
                    let p = PathBuf::from(p);
                    let is_playlist = p
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("m3u8"));
                    if is_playlist {
                        let dir = p
                            .parent()
                            .filter(|d| !d.as_os_str().is_empty())
                            .map(PathBuf::from)
                            .unwrap_or_else(|| PathBuf::from("."));
                        (dir, p)
                    } else {
                        (p.clone(), p.join("index.m3u8"))
                    }
                }
                None => {
                    let dir = folder.join(format.default_file_name());
                    (dir.clone(), dir.join("index.m3u8"))
                }
            };
            ResolvedOutput::Hls { dir, playlist }
        }
        _ => {
            let path = match user_out {
                Some(p) => PathBuf::from(p),
                None => folder.join(format.default_file_name()),
            };
            ResolvedOutput::File { path }
        }
    }
}

/// Staged encode target beside the final destination: a temp file for
/// single files, a temp directory for sequences. Only the finalize
/// step moves staged results into place, so a failed render never
/// leaves a partial file, playlist, or frame behind.
#[derive(Debug, Clone)]
pub enum StagedTarget {
    File {
        /// Temp file FFmpeg writes.
        tmp: PathBuf,
        /// Final file renamed into place on success.
        final_path: PathBuf,
    },
    PngSeq {
        /// Temp directory FFmpeg writes `frame-%04d.png` into.
        stage_dir: PathBuf,
        /// Final directory frames move into on success.
        dir: PathBuf,
        /// Expected frame count (verified before the move).
        frames: usize,
    },
    Hls {
        /// Temp directory FFmpeg writes the playlist plus segments into.
        stage_dir: PathBuf,
        /// Final directory holding the playlist plus segments.
        dir: PathBuf,
        /// Playlist file name (moved under its final name).
        playlist_name: String,
    },
}

/// Temp file beside `final_path` (the historical `.framesmith-part`
/// discipline, so no partial file survives at the target).
pub fn staging_file(final_path: &Path) -> PathBuf {
    let mut name = final_path.as_os_str().to_owned();
    name.push(".framesmith-part");
    PathBuf::from(name)
}

/// Temp directory beside `final_dir` (removed on failure, moved into
/// place file by file on success, so no partial sequence survives).
pub fn staging_dir(final_dir: &Path) -> PathBuf {
    let mut name = final_dir.as_os_str().to_owned();
    name.push(".framesmith-part");
    PathBuf::from(name)
}

impl StagedTarget {
    pub fn stage(dest: &ResolvedOutput, frames: usize) -> StagedTarget {
        match dest {
            ResolvedOutput::File { path } => StagedTarget::File {
                tmp: staging_file(path),
                final_path: path.clone(),
            },
            ResolvedOutput::PngSeq { dir } => StagedTarget::PngSeq {
                stage_dir: staging_dir(dir),
                dir: dir.clone(),
                frames,
            },
            ResolvedOutput::Hls { dir, playlist } => StagedTarget::Hls {
                stage_dir: staging_dir(dir),
                dir: dir.clone(),
                playlist_name: playlist
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "index.m3u8".to_string()),
            },
        }
    }

    /// Every path a failure must remove: the temp file, or the whole
    /// staging directory.
    pub fn work_paths(&self) -> Vec<PathBuf> {
        match self {
            StagedTarget::File { tmp, .. } => vec![tmp.clone()],
            StagedTarget::PngSeq { stage_dir, .. } | StagedTarget::Hls { stage_dir, .. } => {
                vec![stage_dir.clone()]
            }
        }
    }
}

/// Check the host FFmpeg provides the extra video and audio encoders
/// `--format` needs (Sprint 12 outputs, T001): formats reusing the
/// Sprint 1 pair need no check. Refuses before the first frame naming
/// the format, the missing encoder, and the fix.
pub fn check_format_encoders(ffmpeg_bin: &Path, format: OutputFormat) -> Result<(), String> {
    let wanted: Vec<&str> = format
        .required_video_encoder()
        .into_iter()
        .chain(format.required_audio_encoder())
        .collect();
    if wanted.is_empty() {
        return Ok(());
    }
    let text = encoders_text(ffmpeg_bin).map_err(|e| {
        format!(
            "render refused: --format {} needs the {} encoder(s), but the FFmpeg at {} could not be queried ({e}); install an FFmpeg build with {} or pick another --format (validation stage); no output was written",
            format.name(),
            wanted.join(", "),
            ffmpeg_bin.display(),
            wanted.join(", "),
        )
    })?;
    let missing: Vec<&str> = wanted
        .into_iter()
        .filter(|w| !text_has_encoder(&text, w))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(format!(
        "render refused: --format {} needs the {} encoder(s), but the FFmpeg at {} does not provide {}; install an FFmpeg build with {} or pick another --format (validation stage); no output was written",
        format.name(),
        missing.join(", "),
        ffmpeg_bin.display(),
        missing.join(", "),
        missing.join(", "),
    ))
}

/// Output pixel format for the video codec. Every destination renders
/// the opaque canvas (alpha composites at draw time, so pictures match
/// the MP4 exactly): WebM is `yuv420p` because this stack's libvpx drops
/// alpha planes even when `yuva420p` is requested (measured: the encoder
/// converts to `yuv420p` with no error, and VP8 refuses alpha outright),
/// so the argv asks for the opaque format directly.
pub fn video_pix_fmt(format: OutputFormat, profile: ProresProfile) -> Option<&'static str> {
    match format {
        OutputFormat::Mp4 | OutputFormat::Hevc => Some("yuv420p"),
        OutputFormat::Webm => Some("yuv420p"),
        OutputFormat::Prores => Some(profile.pix_fmt()),
        OutputFormat::Gif => None,
        OutputFormat::PngSeq => None,
        OutputFormat::Hls => Some("yuv420p"),
    }
}

/// Audio encoder for the destination: AAC in MP4-family containers,
/// Opus in WebM (the container standard; the native Vorbis encoder is
/// experimental on current FFmpeg builds), PCM in MOV.
pub fn audio_codec(format: OutputFormat) -> Option<&'static str> {
    match format {
        OutputFormat::Mp4 | OutputFormat::Hevc | OutputFormat::Hls => Some("aac"),
        OutputFormat::Webm => Some("libopus"),
        OutputFormat::Prores => Some("pcm_s16le"),
        OutputFormat::Gif | OutputFormat::PngSeq => None,
    }
}

/// Build the full FFmpeg argv for one destination (Sprint 12 outputs,
/// T001/T002/T004): raw `rgb24` frames on stdin in every case, at the output
/// frame size (`frame` is the canvas size on the identity path, the
/// preset size on a scaled render). `mp4` delegates to the pinned
/// standard-MP4 builder so default bytes never drift; every
/// other format reuses the same drawn frames, the same audio filtergraph
/// (when it carries audio), and the same preset axes its encoder honors
/// (`--crf` has no axis on ProRes/GIF/PNG, and ProRes/GIF/PNG never take
/// `-preset`). `target` is already staged (temp file or temp directory
/// pattern), so a failed encode never leaves a partial destination
/// behind.
#[allow(clippy::too_many_arguments)]
pub fn encode_args_for(
    format: OutputFormat,
    profile: ProresProfile,
    comp: &notation::model::Composition,
    audio: &[super::render::AudioSeg],
    mix: Option<&Path>,
    target: &StagedTarget,
    enc: &super::render::ResolvedEncoder,
    duration: f64,
    frame: (u32, u32),
) -> Vec<String> {
    use super::render::{encode_args, encode_input_args, push_audio_args, standard_quiet};
    let feed_audio = format.carries_audio();
    let (audio_segs, audio_mix): (&[super::render::AudioSeg], Option<&Path>) = if feed_audio {
        (audio, mix)
    } else {
        (&[], None)
    };
    // HDR delivery is always HEVC Main10 BT.2020 (Sprint 12 outputs,
    // CAP-004): an HDR MP4 carries HEVC instead of its default H.264,
    // so only the SDR MP4 takes the pinned builder below.
    let hdr = enc.hdr_transfer;
    if format == OutputFormat::Mp4 && hdr == HdrTransfer::Sdr {
        // Pinned: the historical argv exactly (byte-identical default).
        let StagedTarget::File { tmp, .. } = target else {
            panic!("mp4 renders to a file target");
        };
        return encode_args(comp, audio_segs, audio_mix, tmp, enc, duration, frame);
    }
    let mut args = encode_input_args(comp, "rgb24", frame);
    if feed_audio {
        push_audio_args(&mut args, audio_segs, audio_mix, duration);
    } else {
        // Silent-picture destinations encode no audio at all (not even
        // the silent default): `-an` stops the muxer guessing.
        args.push("-an".into());
    }
    // Effective hardware use: only destinations with a hardware
    // encoder honor `enc.hardware` (the caller refuses an explicit
    // `videotoolbox` there; `auto` resolves to software). Anything
    // else falls back to the software codec, so builders never emit
    // a codec the host may lack. HDR always encodes software
    // (libx265 Main10): the hardware encoders tune SDR, so the HDR
    // flags ride the deterministic CPU path.
    let hw_codec: Option<&str> = if enc.hardware && hdr == HdrTransfer::Sdr {
        hardware_encoder_for(format)
    } else {
        None
    };
    // The hardware path drops the x264/x265 quality axes (`-crf`,
    // `-preset`): VideoToolbox tunes quality through bitrate flags
    // instead, and unknown encoder options fail the render.
    let software_quality = hw_codec.is_none() && !standard_quiet(enc);
    // Preset axes only where the encoder honors them. An HDR MP4
    // reaches this builder (never the pinned one), so it rides the
    // HEVC axes exactly like `--format hevc`.
    match format {
        OutputFormat::Hevc | OutputFormat::Hls | OutputFormat::Mp4 => {
            if software_quality {
                args.push("-crf".into());
                args.push(enc.crf.to_string());
                if let Some(p) = enc.x264_preset {
                    args.push("-preset".into());
                    args.push(p.into());
                }
            }
            if let Some(b) = &enc.video_bitrate {
                args.push("-b:v".into());
                args.push(b.clone());
            }
        }
        OutputFormat::Webm => {
            if software_quality {
                args.push("-crf".into());
                args.push(enc.crf.to_string());
            }
            match &enc.video_bitrate {
                // libvpx needs an explicit `-b:v 0` for CRF mode;
                // an explicit cap rides alongside instead.
                Some(b) => {
                    args.push("-b:v".into());
                    args.push(b.clone());
                }
                None => {
                    args.push("-b:v".into());
                    args.push("0".into());
                }
            }
        }
        OutputFormat::Prores => {
            if let Some(b) = &enc.video_bitrate {
                args.push("-b:v".into());
                args.push(b.clone());
            }
        }
        OutputFormat::Gif | OutputFormat::PngSeq => {}
    }
    // Rate control (Sprint 12 outputs, CAP-003): `--keyint` pins the
    // GOP length (`-g`) and `--maxrate` caps peak bitrate (`-maxrate`
    // plus a doubled `-bufsize`) on every destination that writes a
    // video stream. Picture-only destinations (GIF, PNG sequences)
    // carry no stream to pace, so the flags ride silently there.
    // Absent flags emit nothing, keeping default bytes identical.
    if !matches!(format, OutputFormat::Gif | OutputFormat::PngSeq) {
        if let Some(g) = enc.keyint {
            args.push("-g".into());
            args.push(g.to_string());
        }
        if let Some(m) = &enc.maxrate {
            args.push("-maxrate".into());
            args.push(m.clone());
            args.push("-bufsize".into());
            args.push(maxrate_bufsize(m));
        }
    }
    // HDR delivery is 10-bit (Sprint 12 outputs, CAP-004): HEVC
    // Main10 on every format that can carry it. SDR keeps the pinned
    // pixel formats exactly.
    let pix_out = if hdr != HdrTransfer::Sdr
        && matches!(
            format,
            OutputFormat::Mp4 | OutputFormat::Hevc | OutputFormat::Hls
        ) {
        Some("yuv420p10le")
    } else {
        video_pix_fmt(format, profile)
    };
    match format {
        OutputFormat::Hevc | OutputFormat::Mp4 => {
            args.extend([
                "-c:v".into(),
                hw_codec.unwrap_or("libx265").into(),
                "-pix_fmt".into(),
                pix_out.unwrap_or("yuv420p").into(),
                "-tag:v".into(),
                "hvc1".into(),
            ]);
            // BT.2020 Main10 VUI: the probed output carries the
            // transfer's `color_transfer` with BT.2020 primaries and
            // matrix. SDR emits nothing here, keeping bytes identical.
            if hdr != HdrTransfer::Sdr {
                args.extend([
                    "-x265-params".into(),
                    x265_hdr_params(hdr).into(),
                ]);
            }
        }
        OutputFormat::Webm => {
            args.extend([
                "-c:v".into(),
                "libvpx-vp9".into(),
                "-pix_fmt".into(),
                pix_out.unwrap_or("yuv420p").into(),
            ]);
        }
        OutputFormat::Prores => {
            args.extend([
                "-c:v".into(),
                "prores_ks".into(),
                "-profile:v".into(),
                profile.ffmpeg_profile().into(),
                "-pix_fmt".into(),
                pix_out.unwrap_or("yuv422p10le").into(),
            ]);
        }
        OutputFormat::Gif => {
            args.extend([
                "-vf".into(),
                "split[s0][s1];[s0]palettegen=max_colors=256[p];[s1][p]paletteuse".into(),
                "-loop".into(),
                "0".into(),
            ]);
        }
        OutputFormat::PngSeq => {
            let StagedTarget::PngSeq { frames, .. } = target else {
                panic!("pngseq renders to a sequence target");
            };
            args.extend([
                "-frames:v".into(),
                frames.to_string(),
                "-start_number".into(),
                "0".into(),
            ]);
        }
        OutputFormat::Hls => {}
    }
    if format == OutputFormat::Hls {
        // HDR HLS carries HEVC Main10 in its segments; SDR keeps the
        // pinned H.264 exactly.
        if hdr != HdrTransfer::Sdr {
            args.extend([
                "-c:v".into(),
                "libx265".into(),
                "-pix_fmt".into(),
                pix_out.unwrap_or("yuv420p10le").into(),
                "-tag:v".into(),
                "hvc1".into(),
                "-x265-params".into(),
                x265_hdr_params(hdr).into(),
            ]);
        } else {
            args.extend([
                "-c:v".into(),
                hw_codec.unwrap_or("libx264").into(),
                "-pix_fmt".into(),
                pix_out.unwrap_or("yuv420p").into(),
            ]);
        }
    }
    if let Some(acodec) = audio_codec(format) {
        args.extend([
            "-c:a".into(),
            acodec.into(),
            "-ac".into(),
            "2".into(),
            "-ar".into(),
            "48000".into(),
        ]);
    }
    // Provenance (Sprint 12 outputs, CAP-005): containers that carry
    // tags embed the stamp; the tag never touches pixels or samples.
    // PNG sequences ride a sibling sidecar and HLS rides a playlist
    // comment (both written by the caller after finalize), while GIF
    // carries neither (the caller warns naming the format).
    if format_embeds_provenance(format) {
        if let Some(prov) = &enc.provenance {
            args.extend(provenance_metadata_args(&prov.stamp()));
        }
    }
    match target {
        StagedTarget::File { tmp, .. } => {
            // WebM determinism: the Matroska muxer writes random
            // track/segment UIDs unless bitexact is set, while the VP9
            // payload itself is already deterministic — software renders
            // re-render byte-identically.
            if format == OutputFormat::Webm {
                args.extend(["-fflags".into(), "+bitexact".into()]);
            }
            args.extend(["-t".into(), format!("{duration}")]);
            // Every MP4 reaching this builder is HDR (SDR returns
            // pinned), so it faststarts exactly like HEVC.
            if matches!(
                format,
                OutputFormat::Hevc | OutputFormat::Prores | OutputFormat::Mp4
            ) {
                args.extend(["-movflags".into(), "+faststart".into()]);
            }
            // Pinned: the temp file carries a non-standard extension,
            // so the container must not be guessed from its name.
            args.extend([
                "-f".into(),
                format.container().into(),
                tmp.display().to_string(),
            ]);
        }
        StagedTarget::PngSeq { stage_dir, .. } => {
            let pattern = stage_dir.join("frame-%04d.png");
            args.extend(["-f".into(), "image2".into(), pattern.display().to_string()]);
        }
        StagedTarget::Hls {
            stage_dir,
            playlist_name,
            ..
        } => {
            let seg_pattern = stage_dir.join("seg-%03d.ts");
            let playlist = stage_dir.join(playlist_name);
            args.extend([
                "-t".into(),
                format!("{duration}"),
                "-f".into(),
                "hls".into(),
                "-hls_time".into(),
                "2".into(),
                "-hls_list_size".into(),
                "0".into(),
                "-hls_playlist_type".into(),
                "vod".into(),
                "-hls_segment_filename".into(),
                seg_pattern.display().to_string(),
                playlist.display().to_string(),
            ]);
        }
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use notation::model::{Composition, Fps, Rgb};

    fn tiny_comp() -> Composition {
        Composition {
            id: None,
            canvas: None,
            name: "t".to_string(),
            width: 64,
            height: 48,
            duration: 1.0,
            fps: Fps::from_int(30),
            background: Rgb { r: 0, g: 0, b: 0 },
            version: 1,
            clips: Vec::new(),
            fonts: Vec::new(),
            tweens: Vec::new(),
            sets: Vec::new(),
            keys: Vec::new(),
            marks: Vec::new(),
            inputs: Vec::new(),
            variables: Vec::new(),
            replicates: Vec::new(),
            autoplay: false,
            normalize: Default::default(),
            normalize_loc: None,
            grade: notation::model::Grade::default(),
            lane_grades: Vec::new(),
            transitions: Vec::new(),
            motion_blur: None,
        }
    }

    fn standard_enc() -> super::super::render::ResolvedEncoder {
        super::super::render::RenderOptions::default().encoder()
    }

    fn file_target(format: OutputFormat) -> StagedTarget {
        let dest = resolve_output(format, Path::new("proj"), None);
        StagedTarget::stage(&dest, 30)
    }

    #[test]
    fn formats_parse_to_exactly_seven_spellings() {
        for (raw, want) in [
            ("mp4", OutputFormat::Mp4),
            ("hevc", OutputFormat::Hevc),
            ("webm", OutputFormat::Webm),
            ("prores", OutputFormat::Prores),
            ("gif", OutputFormat::Gif),
            ("pngseq", OutputFormat::PngSeq),
            ("hls", OutputFormat::Hls),
        ] {
            assert_eq!(OutputFormat::parse(raw), Some(want), "{raw}");
            assert_eq!(OutputFormat::parse(raw).unwrap().name(), raw);
        }
        for bad in ["", "MP4", "avi", "mov", "av1", " mp4 "] {
            assert_eq!(OutputFormat::parse(bad), None, "{bad:?}");
        }
        assert!(OutputFormat::accepted().contains("pngseq"));
    }

    #[test]
    fn prores_profiles_parse_with_ffmpeg_numbers() {
        for (raw, num, pix) in [
            ("proxy", "0", "yuv422p10le"),
            ("lt", "1", "yuv422p10le"),
            ("standard", "2", "yuv422p10le"),
            ("hq", "3", "yuv422p10le"),
            ("4444", "4", "yuv444p12le"),
        ] {
            let p = ProresProfile::parse(raw).unwrap();
            assert_eq!(p.name(), raw);
            assert_eq!(p.ffmpeg_profile(), num);
            assert_eq!(p.pix_fmt(), pix);
        }
        assert_eq!(ProresProfile::default(), ProresProfile::Standard);
        assert_eq!(ProresProfile::parse("hqx"), None);
    }

    #[test]
    fn destinations_resolve_files_sequences_and_playlists() {
        let folder = Path::new("proj");
        assert_eq!(
            resolve_output(OutputFormat::Mp4, folder, None),
            ResolvedOutput::File {
                path: PathBuf::from("proj/out.mp4")
            }
        );
        assert_eq!(
            resolve_output(OutputFormat::Webm, folder, None),
            ResolvedOutput::File {
                path: PathBuf::from("proj/out.webm")
            }
        );
        assert_eq!(
            resolve_output(OutputFormat::Prores, folder, None),
            ResolvedOutput::File {
                path: PathBuf::from("proj/out.mov")
            }
        );
        assert_eq!(
            resolve_output(OutputFormat::Gif, folder, None),
            ResolvedOutput::File {
                path: PathBuf::from("proj/out.gif")
            }
        );
        assert_eq!(
            resolve_output(OutputFormat::Hevc, folder, Some(Path::new("custom.mp4"))),
            ResolvedOutput::File {
                path: PathBuf::from("custom.mp4")
            }
        );
        assert_eq!(
            resolve_output(OutputFormat::PngSeq, folder, None),
            ResolvedOutput::PngSeq {
                dir: PathBuf::from("proj/frames")
            }
        );
        assert_eq!(
            resolve_output(OutputFormat::Hls, folder, None),
            ResolvedOutput::Hls {
                dir: PathBuf::from("proj/hls"),
                playlist: PathBuf::from("proj/hls/index.m3u8"),
            }
        );
        assert_eq!(
            resolve_output(
                OutputFormat::Hls,
                folder,
                Some(Path::new("proj/stream.m3u8"))
            ),
            ResolvedOutput::Hls {
                dir: PathBuf::from("proj"),
                playlist: PathBuf::from("proj/stream.m3u8"),
            }
        );
    }

    #[test]
    fn mp4_argv_matches_the_pinned_standard_builder() {
        let comp = tiny_comp();
        let target = file_target(OutputFormat::Mp4);
        let args = encode_args_for(
            OutputFormat::Mp4,
            ProresProfile::Standard,
            &comp,
            &[],
            None,
            &target,
            &standard_enc(),
            comp.duration,
            (comp.width, comp.height),
        );
        let joined = args.join(" ");
        for flag in [
            "libx264", "yuv420p", "aac", "48000", "rawvideo", "-f", "mp4",
        ] {
            assert!(joined.contains(flag), "{joined}");
        }
        assert!(!joined.contains("-crf"), "{joined}");
    }

    #[test]
    fn hevc_argv_carries_h265_in_mp4() {
        let comp = tiny_comp();
        let target = file_target(OutputFormat::Hevc);
        let args = encode_args_for(
            OutputFormat::Hevc,
            ProresProfile::Standard,
            &comp,
            &[],
            None,
            &target,
            &standard_enc(),
            comp.duration,
            (comp.width, comp.height),
        );
        let joined = args.join(" ");
        for flag in ["libx265", "yuv420p", "hvc1", "aac", "+faststart", "-f mp4"] {
            assert!(joined.contains(flag), "{joined}");
        }
        assert!(!joined.contains("libx264"), "{joined}");
        let draft = super::super::render::RenderOptions {
            preset: super::super::render::Preset::Draft,
            ..super::super::render::RenderOptions::default()
        }
        .encoder();
        let args = encode_args_for(
            OutputFormat::Hevc,
            ProresProfile::Standard,
            &comp,
            &[],
            None,
            &target,
            &draft,
            comp.duration,
            (comp.width, comp.height),
        );
        let joined = args.join(" ");
        assert!(joined.contains("-crf 30"), "{joined}");
        assert!(joined.contains("-preset veryfast"), "{joined}");
    }

    #[test]
    fn webm_argv_encodes_opaque_pictures_in_crf_mode() {
        // Alpha composites at draw time, so WebM carries the same
        // opaque pictures as every other destination; libvpx drops
        // alpha planes even when requested, so the argv asks for
        // `yuv420p` directly.
        let comp = tiny_comp();
        let target = file_target(OutputFormat::Webm);
        let args = encode_args_for(
            OutputFormat::Webm,
            ProresProfile::Standard,
            &comp,
            &[],
            None,
            &target,
            &standard_enc(),
            comp.duration,
            (comp.width, comp.height),
        );
        let joined = args.join(" ");
        for flag in [
            "libvpx-vp9",
            "yuv420p",
            "libopus",
            "-b:v 0",
            "-f webm",
            "+bitexact",
        ] {
            assert!(joined.contains(flag), "{joined}");
        }
        assert!(!joined.contains("yuva"), "{joined}");
        assert!(!joined.contains("-preset"), "{joined}");
    }

    #[test]
    fn prores_argv_honors_every_profile() {
        let comp = tiny_comp();
        for (profile, num, pix) in [
            (ProresProfile::Proxy, "0", "yuv422p10le"),
            (ProresProfile::Lt, "1", "yuv422p10le"),
            (ProresProfile::Standard, "2", "yuv422p10le"),
            (ProresProfile::Hq, "3", "yuv422p10le"),
            (ProresProfile::P4444, "4", "yuv444p12le"),
        ] {
            let target = file_target(OutputFormat::Prores);
            let args = encode_args_for(
                OutputFormat::Prores,
                profile,
                &comp,
                &[],
                None,
                &target,
                &standard_enc(),
                comp.duration,
                (comp.width, comp.height),
            );
            let joined = args.join(" ");
            for flag in ["prores_ks", "pcm_s16le", "-f mov"] {
                assert!(joined.contains(flag), "{joined}");
            }
            assert!(joined.contains(&format!("-profile:v {num}")), "{joined}");
            assert!(joined.contains(pix), "{joined}");
            assert!(!joined.contains("-crf"), "{joined}");
            assert!(!joined.contains("-preset "), "{joined}");
        }
    }

    #[test]
    fn gif_argv_loops_with_a_fixed_palette_and_no_audio() {
        let comp = tiny_comp();
        let target = file_target(OutputFormat::Gif);
        let args = encode_args_for(
            OutputFormat::Gif,
            ProresProfile::Standard,
            &comp,
            &[],
            None,
            &target,
            &standard_enc(),
            comp.duration,
            (comp.width, comp.height),
        );
        let joined = args.join(" ");
        assert!(joined.contains("palettegen"), "{joined}");
        assert!(joined.contains("paletteuse"), "{joined}");
        assert!(joined.contains("-loop 0"), "{joined}");
        assert!(joined.contains("-f gif"), "{joined}");
        assert!(!joined.contains("-c:a"), "{joined}");
        assert!(joined.contains("-an"), "{joined}");
        assert!(!joined.contains("anullsrc"), "{joined}");
    }

    #[test]
    fn pngseq_argv_writes_numbered_frames_without_audio() {
        let comp = tiny_comp();
        let dest = resolve_output(OutputFormat::PngSeq, Path::new("proj"), None);
        let target = StagedTarget::stage(&dest, 30);
        let args = encode_args_for(
            OutputFormat::PngSeq,
            ProresProfile::Standard,
            &comp,
            &[],
            None,
            &target,
            &standard_enc(),
            comp.duration,
            (comp.width, comp.height),
        );
        let joined = args.join(" ");
        assert!(joined.contains("-frames:v 30"), "{joined}");
        assert!(joined.contains("frame-%04d.png"), "{joined}");
        assert!(joined.contains("-f image2"), "{joined}");
        assert!(!joined.contains("-c:a"), "{joined}");
        assert!(joined.contains("-an"), "{joined}");
    }

    #[test]
    fn hls_argv_writes_a_vod_playlist_with_h264_and_aac() {
        let comp = tiny_comp();
        let dest = resolve_output(OutputFormat::Hls, Path::new("proj"), None);
        let target = StagedTarget::stage(&dest, 30);
        let args = encode_args_for(
            OutputFormat::Hls,
            ProresProfile::Standard,
            &comp,
            &[],
            None,
            &target,
            &standard_enc(),
            comp.duration,
            (comp.width, comp.height),
        );
        let joined = args.join(" ");
        for flag in [
            "libx264",
            "yuv420p",
            "aac",
            "-f hls",
            "-hls_time 2",
            "-hls_playlist_type vod",
            "seg-%03d.ts",
            "index.m3u8",
        ] {
            assert!(joined.contains(flag), "{joined}");
        }
    }

    #[test]
    fn encoder_check_passes_on_formats_and_names_missing_encoders() {
        let ffmpeg = PathBuf::from("ffmpeg");
        for format in [
            OutputFormat::Mp4,
            OutputFormat::Hevc,
            OutputFormat::Webm,
            OutputFormat::Prores,
            OutputFormat::Gif,
            OutputFormat::PngSeq,
            OutputFormat::Hls,
        ] {
            assert!(
                check_format_encoders(&ffmpeg, format).is_ok(),
                "{:?}",
                format.name()
            );
        }
        let missing = PathBuf::from("/nonexistent-ffmpeg-binary");
        let err = check_format_encoders(&missing, OutputFormat::Hevc).unwrap_err();
        assert!(err.contains("--format hevc"), "{err}");
        assert!(err.contains("libx265"), "{err}");
    }

    #[test]
    fn encoder_choice_parses_to_exactly_three_spellings() {
        use super::EncoderChoice;
        for (raw, want) in [
            ("auto", EncoderChoice::Auto),
            ("software", EncoderChoice::Software),
            ("videotoolbox", EncoderChoice::Videotoolbox),
        ] {
            assert_eq!(EncoderChoice::parse(raw), Some(want), "{raw}");
            assert_eq!(EncoderChoice::parse(raw).unwrap().name(), raw);
        }
        for bad in ["", "AUTO", "cuda", "gpu", " auto "] {
            assert_eq!(EncoderChoice::parse(bad), None, "{bad:?}");
        }
        assert_eq!(EncoderChoice::default(), EncoderChoice::Software);
        assert!(EncoderChoice::accepted().contains("videotoolbox"));
    }

    #[test]
    fn keyint_accepts_only_positive_whole_frames() {
        assert_eq!(parse_keyint("1"), Some(1));
        assert_eq!(parse_keyint("60"), Some(60));
        assert_eq!(parse_keyint(" 30 "), Some(30));
        for bad in ["0", "-4", "1.5", "many", "", "   ", "0x10", "4294967296"] {
            assert_eq!(parse_keyint(bad), None, "{bad:?}");
        }
        assert!(keyint_accepted().contains("1 or more"));
    }

    #[test]
    fn maxrate_accepts_bitrates_and_doubles_the_bufsize() {
        assert_eq!(parse_maxrate("8M").as_deref(), Some("8M"));
        assert_eq!(parse_maxrate("800k").as_deref(), Some("800k"));
        assert_eq!(parse_maxrate("2m").as_deref(), Some("2m"));
        assert_eq!(parse_maxrate("500000").as_deref(), Some("500000"));
        assert_eq!(parse_maxrate(" 1.5M ").as_deref(), Some("1.5M"));
        for bad in ["fast", "-5M", "0", "0k", "", "   ", "8G", "M"] {
            assert_eq!(parse_maxrate(bad), None, "{bad:?}");
        }
        assert_eq!(maxrate_bufsize("8M"), "16M");
        assert_eq!(maxrate_bufsize("800k"), "1600k");
        assert_eq!(maxrate_bufsize("2m"), "4m");
        assert_eq!(maxrate_bufsize("500000"), "1000000");
        assert_eq!(maxrate_bufsize("1.5M"), "3M");
        assert!(maxrate_accepted().contains("8M"));
    }

    #[test]
    fn hardware_encoders_cover_only_the_h264_h265_destinations() {
        assert_eq!(
            hardware_encoder_for(OutputFormat::Mp4),
            Some("h264_videotoolbox")
        );
        assert_eq!(
            hardware_encoder_for(OutputFormat::Hls),
            Some("h264_videotoolbox")
        );
        assert_eq!(
            hardware_encoder_for(OutputFormat::Hevc),
            Some("hevc_videotoolbox")
        );
        for format in [
            OutputFormat::Webm,
            OutputFormat::Prores,
            OutputFormat::Gif,
            OutputFormat::PngSeq,
        ] {
            assert_eq!(hardware_encoder_for(format), None, "{}", format.name());
        }
        // Destinations without a hardware encoder never report
        // available, whatever the host ships.
        let ffmpeg = PathBuf::from("ffmpeg");
        for format in [
            OutputFormat::Webm,
            OutputFormat::Prores,
            OutputFormat::Gif,
            OutputFormat::PngSeq,
        ] {
            assert!(!hardware_available(&ffmpeg, format), "{}", format.name());
        }
        // The absence hook hides hardware even where present.
        std::env::set_var("FRAMESMITH_NO_VIDEOTOOLBOX", "1");
        assert!(!hardware_available(&ffmpeg, OutputFormat::Mp4));
        assert!(!hardware_available(&ffmpeg, OutputFormat::Hevc));
        std::env::remove_var("FRAMESMITH_NO_VIDEOTOOLBOX");
    }

    #[test]
    fn rate_control_argv_paces_video_streams_only() {
        use super::super::render::RenderOptions;
        let comp = tiny_comp();
        let paced = RenderOptions {
            keyint: Some(5),
            maxrate: Some("800k".to_string()),
            ..RenderOptions::default()
        }
        .encoder();
        // HEVC carries the interval and the cap with a doubled bufsize.
        let target = file_target(OutputFormat::Hevc);
        let args = encode_args_for(
            OutputFormat::Hevc,
            ProresProfile::Standard,
            &comp,
            &[],
            None,
            &target,
            &paced,
            comp.duration,
            (comp.width, comp.height),
        );
        let joined = args.join(" ");
        assert!(joined.contains("-g 5"), "{joined}");
        assert!(joined.contains("-maxrate 800k"), "{joined}");
        assert!(joined.contains("-bufsize 1600k"), "{joined}");
        assert!(joined.contains("libx265"), "{joined}");
        // The default path emits no pacing flags at all.
        let target = file_target(OutputFormat::Hevc);
        let args = encode_args_for(
            OutputFormat::Hevc,
            ProresProfile::Standard,
            &comp,
            &[],
            None,
            &target,
            &standard_enc(),
            comp.duration,
            (comp.width, comp.height),
        );
        let joined = args.join(" ");
        assert!(!joined.contains("-g "), "{joined}");
        assert!(!joined.contains("-maxrate"), "{joined}");
        assert!(!joined.contains("-bufsize"), "{joined}");
        // Picture-only destinations ignore pacing silently.
        let dest = resolve_output(OutputFormat::PngSeq, Path::new("proj"), None);
        let target = StagedTarget::stage(&dest, 30);
        let args = encode_args_for(
            OutputFormat::PngSeq,
            ProresProfile::Standard,
            &comp,
            &[],
            None,
            &target,
            &paced,
            comp.duration,
            (comp.width, comp.height),
        );
        let joined = args.join(" ");
        assert!(!joined.contains("-g "), "{joined}");
        assert!(!joined.contains("-maxrate"), "{joined}");
    }

    #[test]
    fn hdr_modes_parse_to_exactly_four_spellings() {
        for (raw, want) in [
            ("off", HdrMode::Off),
            ("auto", HdrMode::Auto),
            ("pq", HdrMode::Pq),
            ("hlg", HdrMode::Hlg),
        ] {
            assert_eq!(HdrMode::parse(raw), Some(want), "{raw}");
            assert_eq!(HdrMode::parse(raw).unwrap().name(), raw);
        }
        for bad in ["", "AUTO", "hdr", "sdr", "dolby", " auto "] {
            assert_eq!(HdrMode::parse(bad), None, "{bad:?}");
        }
        assert_eq!(HdrMode::default(), HdrMode::Auto);
        assert!(HdrMode::accepted().contains("pq"));
    }

    #[test]
    fn transfer_detects_pq_hlg_and_sdr_from_probe_text() {
        let pq = "Stream #0:0: Video: hevc (Main 10), yuv420p10le(tv, bt2020nc/bt2020/smpte2084), 64x48";
        let hlg = "Stream #0:0: Video: hevc (Main 10), yuv420p10le(tv, bt2020nc/bt2020/arib-std-b67), 64x48";
        let sdr = "Stream #0:0: Video: h264 (High), yuv420p, 160x120";
        let mastering_only =
            "Side data: Mastering display metadata: smpte2086 nonsense, no transfer named";
        assert_eq!(detect_transfer(pq), HdrTransfer::Pq);
        assert_eq!(detect_transfer(hlg), HdrTransfer::Hlg);
        assert_eq!(detect_transfer(sdr), HdrTransfer::Sdr);
        // `smpte2086` names the mastering display, never the transfer.
        assert_eq!(detect_transfer(mastering_only), HdrTransfer::Sdr);
        assert_eq!(detect_transfer(""), HdrTransfer::Sdr);
        // A confused container naming both resolves to PQ.
        assert_eq!(
            detect_transfer(&format!("{pq} plus {hlg}")),
            HdrTransfer::Pq
        );
    }

    #[test]
    fn only_mp4_family_and_hls_carry_hdr() {
        for format in [
            OutputFormat::Mp4,
            OutputFormat::Hevc,
            OutputFormat::Hls,
        ] {
            assert!(format_carries_hdr(format), "{}", format.name());
            assert!(!format_tone_maps_hdr(format), "{}", format.name());
        }
        for format in [OutputFormat::Gif, OutputFormat::PngSeq] {
            assert!(!format_carries_hdr(format), "{}", format.name());
            assert!(format_tone_maps_hdr(format), "{}", format.name());
        }
        for format in [OutputFormat::Webm, OutputFormat::Prores] {
            assert!(!format_carries_hdr(format), "{}", format.name());
            assert!(!format_tone_maps_hdr(format), "{}", format.name());
        }
        assert_eq!(
            x265_hdr_params(HdrTransfer::Pq),
            "colorprim=bt2020:transfer=smpte2084:colormatrix=bt2020nc"
        );
        assert_eq!(
            x265_hdr_params(HdrTransfer::Hlg),
            "colorprim=bt2020:transfer=arib-std-b67:colormatrix=bt2020nc"
        );
    }

    #[test]
    fn hdr_argv_carries_main10_bt2020_on_hevc_destinations() {
        use super::super::render::RenderOptions;
        let comp = tiny_comp();
        for (format, transfer, want_trc) in [
            (OutputFormat::Hevc, HdrTransfer::Pq, "transfer=smpte2084"),
            (OutputFormat::Hevc, HdrTransfer::Hlg, "transfer=arib-std-b67"),
            (OutputFormat::Mp4, HdrTransfer::Pq, "transfer=smpte2084"),
            (OutputFormat::Hls, HdrTransfer::Hlg, "transfer=arib-std-b67"),
        ] {
            let hdr_opts = RenderOptions {
                format,
                hdr_transfer: transfer,
                ..RenderOptions::default()
            };
            let enc = hdr_opts.encoder();
            let dest = match format {
                OutputFormat::Hls => {
                    let d = resolve_output(format, Path::new("proj"), None);
                    StagedTarget::stage(&d, 30)
                }
                _ => file_target(format),
            };
            let args = encode_args_for(
                format,
                ProresProfile::Standard,
                &comp,
                &[],
                None,
                &dest,
                &enc,
                comp.duration,
                (comp.width, comp.height),
            );
            let joined = args.join(" ");
            for flag in ["libx265", "yuv420p10le", want_trc, "colorprim=bt2020"] {
                assert!(joined.contains(flag), "{format:?} {transfer:?}: {joined}");
            }
            assert!(!joined.contains("libx264"), "{joined}");
            assert!(!joined.contains("yuv420p,"), "{joined}");
        }
    }

    #[test]
    fn sdr_argv_emits_no_hdr_flags() {
        let comp = tiny_comp();
        for format in [
            OutputFormat::Mp4,
            OutputFormat::Hevc,
            OutputFormat::Hls,
            OutputFormat::Webm,
        ] {
            let target = file_target(format);
            let args = encode_args_for(
                format,
                ProresProfile::Standard,
                &comp,
                &[],
                None,
                &target,
                &standard_enc(),
                comp.duration,
                (comp.width, comp.height),
            );
            let joined = args.join(" ");
            assert!(!joined.contains("yuv420p10le"), "{format:?}: {joined}");
            assert!(!joined.contains("x265-params"), "{format:?}: {joined}");
            assert!(!joined.contains("bt2020"), "{format:?}: {joined}");
        }
    }

    #[test]
    fn hardware_argv_selects_videotoolbox_without_x264_axes() {
        use super::super::render::RenderOptions;
        let comp = tiny_comp();
        let hw = RenderOptions {
            encoder_choice: super::EncoderChoice::Videotoolbox,
            hardware: true,
            ..RenderOptions::default()
        }
        .encoder();
        let target = file_target(OutputFormat::Hls);
        let args = encode_args_for(
            OutputFormat::Hls,
            ProresProfile::Standard,
            &comp,
            &[],
            None,
            &target,
            &hw,
            comp.duration,
            (comp.width, comp.height),
        );
        let joined = args.join(" ");
        assert!(joined.contains("h264_videotoolbox"), "{joined}");
        assert!(!joined.contains("libx264"), "{joined}");
        assert!(!joined.contains("-crf"), "{joined}");
        assert!(!joined.contains("-preset"), "{joined}");
        // A hardware request on a destination without a hardware
        // encoder falls back to the software codec, never a missing one.
        let target = file_target(OutputFormat::Webm);
        let args = encode_args_for(
            OutputFormat::Webm,
            ProresProfile::Standard,
            &comp,
            &[],
            None,
            &target,
            &hw,
            comp.duration,
            (comp.width, comp.height),
        );
        let joined = args.join(" ");
        assert!(joined.contains("libvpx-vp9"), "{joined}");
        assert!(!joined.contains("videotoolbox"), "{joined}");
    }

    fn test_provenance() -> Provenance {
        Provenance {
            version: "0.1.0".to_string(),
            digest: "9e3779b97f4a7c15".to_string(),
        }
    }

    #[test]
    fn provenance_stamp_names_tool_version_and_digest() {
        let prov = test_provenance();
        assert_eq!(Provenance::tool(), "framesmith");
        let stamp = prov.stamp();
        assert!(stamp.starts_with("framesmith 0.1.0 inputs="), "{stamp}");
        assert!(stamp.contains("9e3779b97f4a7c15"), "{stamp}");
        let sidecar = prov.sidecar_json();
        assert!(sidecar.contains("\"tool\":\"framesmith\""), "{sidecar}");
        assert!(sidecar.contains("\"version\":\"0.1.0\""), "{sidecar}");
        assert!(sidecar.contains("\"digest\":\"9e3779b97f4a7c15\""), "{sidecar}");
        assert_eq!(
            provenance_sidecar_for_seq(Path::new("proj/frames")),
            PathBuf::from("proj/frames.provenance.json")
        );
        assert_eq!(
            provenance_sidecar_for_seq(Path::new("stills")),
            PathBuf::from("stills.provenance.json")
        );
        assert_eq!(provenance_playlist_comment(&stamp), format!("# {stamp}"));
    }

    #[test]
    fn provenance_digest_is_stable_sorted_and_sensitive() {
        let files = vec![
            ("b.wav".to_string(), b"bbbb".to_vec()),
            ("a.mp4".to_string(), b"aaaa".to_vec()),
        ];
        let once = provenance_digest("canon", &files);
        let mut swapped = files.clone();
        swapped.reverse();
        // Call order never changes the digest.
        assert_eq!(provenance_digest("canon", &swapped), once);
        assert_eq!(once.len(), 16, "{once}");
        assert!(once.chars().all(|c| c.is_ascii_hexdigit()), "{once}");
        // Any input change re-stamps.
        assert_ne!(provenance_digest("canon!", &files), once);
        assert_ne!(
            provenance_digest("canon", &[("a.mp4".to_string(), b"aaab".to_vec())]),
            once
        );
        assert_ne!(provenance_digest("canon", &[]), once);
    }

    #[test]
    fn only_file_containers_embed_the_stamp() {
        for format in [
            OutputFormat::Mp4,
            OutputFormat::Hevc,
            OutputFormat::Webm,
            OutputFormat::Prores,
        ] {
            assert!(format_embeds_provenance(format), "{}", format.name());
        }
        for format in [
            OutputFormat::Gif,
            OutputFormat::PngSeq,
            OutputFormat::Hls,
        ] {
            assert!(!format_embeds_provenance(format), "{}", format.name());
        }
        let args = provenance_metadata_args("framesmith 0.1.0 inputs=abc");
        assert_eq!(
            args,
            vec![
                "-metadata".to_string(),
                "comment=framesmith 0.1.0 inputs=abc".to_string()
            ]
        );
    }

    #[test]
    fn stamped_argv_carries_metadata_without_changing_codecs() {
        use super::super::render::RenderOptions;
        let comp = tiny_comp();
        let stamped_opts = RenderOptions {
            provenance: Some(test_provenance()),
            ..RenderOptions::default()
        };
        let enc = stamped_opts.encoder();
        assert_eq!(enc.provenance, Some(test_provenance()));
        // The default encoder stays unstamped, keeping pinned argv exact.
        assert_eq!(standard_enc().provenance, None);
        for format in [
            OutputFormat::Mp4,
            OutputFormat::Hevc,
            OutputFormat::Webm,
            OutputFormat::Prores,
        ] {
            let target = file_target(format);
            let args = encode_args_for(
                format,
                ProresProfile::Standard,
                &comp,
                &[],
                None,
                &target,
                &enc,
                comp.duration,
                (comp.width, comp.height),
            );
            let joined = args.join(" ");
            assert!(
                joined.contains("comment=framesmith 0.1.0 inputs=9e3779b97f4a7c15"),
                "{format:?}: {joined}"
            );
        }
        // Picture-only and playlist destinations carry no container tag.
        let dest = resolve_output(OutputFormat::PngSeq, Path::new("proj"), None);
        let target = StagedTarget::stage(&dest, 30);
        let args = encode_args_for(
            OutputFormat::PngSeq,
            ProresProfile::Standard,
            &comp,
            &[],
            None,
            &target,
            &enc,
            comp.duration,
            (comp.width, comp.height),
        );
        assert!(!args.join(" ").contains("comment="));
        let target = file_target(OutputFormat::Gif);
        let args = encode_args_for(
            OutputFormat::Gif,
            ProresProfile::Standard,
            &comp,
            &[],
            None,
            &target,
            &enc,
            comp.duration,
            (comp.width, comp.height),
        );
        assert!(!args.join(" ").contains("comment="));
        // Unstamped renders emit no metadata flag at all.
        let target = file_target(OutputFormat::Hevc);
        let args = encode_args_for(
            OutputFormat::Hevc,
            ProresProfile::Standard,
            &comp,
            &[],
            None,
            &target,
            &standard_enc(),
            comp.duration,
            (comp.width, comp.height),
        );
        assert!(!args.join(" ").contains("-metadata"));
    }
}
