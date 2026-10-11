//! The facts of a video file, read with `ffprobe`.

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    pub video_streams: usize,
    pub audio_streams: usize,
    /// Of the first video stream, chosen by type, not by position.
    pub codec: String,
    pub pixel_format: String,
    pub width: u32,
    pub height: u32,
    /// The frame rate as written in the file, for example `30/1`.
    pub frame_rate: String,
    /// Frames counted by decoding.
    pub frames: u64,
    /// The length of the video stream in seconds.
    pub duration: f64,
    /// Whether the index a player needs comes before the picture data.
    pub index_first: bool,
    /// Every other fact of the stream, for information.
    pub other: BTreeMap<String, String>,
}

/// The order of the top-level boxes of an MP4 file.
fn top_boxes(bytes: &[u8]) -> Vec<String> {
    let mut boxes = Vec::new();
    let mut at = 0usize;
    while let (Some(size), Some(kind)) = (bytes.get(at..at + 4), bytes.get(at + 4..at + 8)) {
        let mut size = u32::from_be_bytes([size[0], size[1], size[2], size[3]]) as u64;
        if size == 1 {
            let Some(large) = bytes.get(at + 8..at + 16) else { break };
            size = u64::from_be_bytes(large.try_into().unwrap_or_default());
        }
        boxes.push(String::from_utf8_lossy(kind).into_owned());
        if size < 8 {
            break;
        }
        at = at.saturating_add(size as usize);
    }
    boxes
}

pub fn facts(ffprobe: &Path, video: &Path) -> Result<Facts, String> {
    let output = Command::new(ffprobe)
        .args(["-v", "error", "-count_frames", "-show_streams", "-of", "json"])
        .arg(video)
        .output()
        .map_err(|e| format!("{} could not be run: {e}", ffprobe.display()))?;
    if !output.status.success() {
        return Err(format!(
            "ffprobe could not read {}: {}",
            video.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let parsed: Value = serde_json::from_slice(&output.stdout).map_err(|e| format!("ffprobe's answer could not be read ({e})"))?;
    let streams = parsed["streams"].as_array().cloned().unwrap_or_default();
    let of_type = |kind: &str| streams.iter().filter(|s| s["codec_type"] == kind).count();
    let mut facts = Facts {
        video_streams: of_type("video"),
        audio_streams: of_type("audio"),
        ..Facts::default()
    };
    let Some(stream) = streams.iter().find(|s| s["codec_type"] == "video") else {
        return Err(format!("{} has no video stream", video.display()));
    };
    let text = |name: &str| stream[name].as_str().unwrap_or_default().to_string();
    facts.codec = text("codec_name");
    facts.pixel_format = text("pix_fmt");
    facts.width = stream["width"].as_u64().unwrap_or(0) as u32;
    facts.height = stream["height"].as_u64().unwrap_or(0) as u32;
    facts.frame_rate = text("r_frame_rate");
    facts.frames = text("nb_read_frames").parse().unwrap_or(0);
    facts.duration = text("duration").parse().unwrap_or(0.0);
    for name in [
        "profile",
        "level",
        "color_range",
        "color_space",
        "color_transfer",
        "color_primaries",
        "time_base",
        "avg_frame_rate",
        "bit_rate",
        "has_b_frames",
    ] {
        let value = &stream[name];
        let shown = value.as_str().map_or_else(|| value.to_string(), str::to_string);
        if !value.is_null() {
            facts.other.insert(name.to_string(), shown);
        }
    }
    let bytes = std::fs::read(video).map_err(|e| format!("{} could not be read ({e})", video.display()))?;
    let boxes = top_boxes(&bytes);
    let place = |kind: &str| boxes.iter().position(|b| b == kind);
    facts.index_first = matches!((place("moov"), place("mdat")), (Some(index), Some(data)) if index < data);
    Ok(facts)
}
