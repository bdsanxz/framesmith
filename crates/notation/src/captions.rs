//! Caption files, timing, styles, and presets (Sprint 22 captions,
//! T001/T002/T003 / CAP-001..CAP-005).
//!
//! Pure standard-library-plus-`serde_json` logic shared by validation
//! (decoders and gates) and render (seeking, presets, export): no
//! browser, no JavaScript, no network at any step. Every time map is
//! a pure function of film time, so reruns agree byte for byte.
//!
//! Cue and word times are clip-local seconds (offset by the clip
//! `start` at render). Decoders quantize every time to whole
//! milliseconds on the way in, so an export followed by a re-import
//! reproduces the track with zero timing drift.

use crate::model::{CaptionCue, CaptionWord};

/// Whole-millisecond quantum every decoded time snaps to: decoders
/// round to this grid, exporters print it exactly, so a re-imported
/// export matches the track with zero drift.
pub const TIME_QUANTUM: f64 = 0.001;

/// Snap a decoded time to the millisecond grid (finite inputs only;
/// non-finite times are refused before this runs).
pub fn quantize_ms(t: f64) -> f64 {
    (t / TIME_QUANTUM).round() * TIME_QUANTUM
}

/// One caption decode failure: the 1-based file line (0 for whole
/// body failures) plus what was wrong there.
#[derive(Debug, Clone, PartialEq)]
pub struct CaptionParseError {
    pub line: usize,
    pub detail: String,
}

impl CaptionParseError {
    fn at(line: usize, detail: impl Into<String>) -> Self {
        Self {
            line,
            detail: detail.into(),
        }
    }

    fn body(detail: impl Into<String>) -> Self {
        Self {
            line: 0,
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for CaptionParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line == 0 {
            write!(f, "{}", self.detail)
        } else {
            write!(f, "line {}: {}", self.line, self.detail)
        }
    }
}

/// One decoded track: cues plus the non-blocking notes validation
/// surfaces by name (dropped zero-length words, empty cues kept as
/// silent placeholders).
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedTrack {
    pub cues: Vec<CaptionCue>,
    pub notes: Vec<String>,
}

/// Parse `HH:MM:SS,mmm`, `HH:MM:SS.mmm`, or `MM:SS.mmm` (hours may
/// exceed two digits) into seconds. Anything else is refused.
pub fn parse_timestamp(raw: &str) -> Option<f64> {
    let raw = raw.trim().replace(',', ".");
    let parts: Vec<&str> = raw.split(':').collect();
    let (h, m, s) = match parts.as_slice() {
        [mm, ss] => (0.0, mm.parse::<f64>().ok()?, ss.parse::<f64>().ok()?),
        [hh, mm, ss] => (
            hh.parse::<f64>().ok()?,
            mm.parse::<f64>().ok()?,
            ss.parse::<f64>().ok()?,
        ),
        _ => return None,
    };
    if h < 0.0 || m < 0.0 || m >= 60.0 || s < 0.0 || s >= 60.0 {
        return None;
    }
    Some(h * 3600.0 + m * 60.0 + s)
}

/// Print seconds as `HH:MM:SS,mmm` (SRT) or `HH:MM:SS.mmm` (VTT),
/// clamped at zero and rounded to the millisecond grid.
pub fn format_timestamp(t: f64, comma: bool) -> String {
    let total_ms = (t.max(0.0) / TIME_QUANTUM).round() as u64;
    let ms = total_ms % 1000;
    let total_s = total_ms / 1000;
    let s = total_s % 60;
    let total_m = total_s / 60;
    let m = total_m % 60;
    let h = total_m / 60;
    let sep = if comma { ',' } else { '.' };
    format!("{h:02}:{m:02}:{s:02}{sep}{ms:03}")
}

/// Split cue text into words spread uniformly across `[start, end)`:
/// the deterministic word timing SRT and VTT import through (they
/// carry no per-word times). Zero-length words are dropped with a
/// named note; an empty text yields an empty cue that renders
/// nothing and never invents words.
/// Spread cue text into words uniformly across `[start, end)`
/// (Sprint 22 captions, T001): the deterministic word timing SRT and
/// VTT imports (and valueless inline cues) share. Zero-length words
/// are dropped with a named note; empty text yields no words.
pub fn uniform_words(
    text: &str,
    start: f64,
    end: f64,
    cue_no: usize,
    notes: &mut Vec<String>,
) -> Vec<CaptionWord> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }
    let span = (end - start) / words.len() as f64;
    let mut out = Vec::with_capacity(words.len());
    for (i, w) in words.iter().enumerate() {
        let ws = quantize_ms(start + span * i as f64);
        let we = quantize_ms(start + span * (i + 1) as f64);
        if we <= ws {
            notes.push(format!(
                "cue {cue_no} word {i} {w:?} has zero length and was dropped"
            ));
            continue;
        }
        out.push(CaptionWord {
            text: w.to_string(),
            start: ws,
            end: we.min(end),
        });
    }
    out
}

/// Shared cue-window checks: `start < end`, both finite and
/// non-negative, words inside the window in order. Refusals name the
/// cue index (1-based, matching the file order makers read).
pub fn check_cues(cues: &[CaptionCue]) -> Result<(), CaptionParseError> {
    for (i, cue) in cues.iter().enumerate() {
        let no = i + 1;
        if !cue.start.is_finite() || !cue.end.is_finite() {
            return Err(CaptionParseError::at(
                cue.line,
                format!("cue {no} carries a non-finite time"),
            ));
        }
        if cue.start < 0.0 {
            return Err(CaptionParseError::at(
                cue.line,
                format!("cue {no} starts at {:.3}s, before zero", cue.start),
            ));
        }
        if cue.end <= cue.start {
            return Err(CaptionParseError::at(
                cue.line,
                format!(
                    "cue {no} ends at {:.3}s, not past its {:.3}s start",
                    cue.end, cue.start
                ),
            ));
        }
        for (j, w) in cue.words.iter().enumerate() {
            if !w.start.is_finite() || !w.end.is_finite() || w.end <= w.start {
                return Err(CaptionParseError::at(
                    cue.line,
                    format!("cue {no} word {j} {:?} carries unusable times", w.text),
                ));
            }
            if w.start < cue.start - 1e-9 || w.end > cue.end + 1e-9 {
                return Err(CaptionParseError::at(
                    cue.line,
                    format!(
                        "cue {no} word {j} {:?} [{:.3}s, {:.3}s) falls outside its cue [{:.3}s, {:.3}s)",
                        w.text, w.start, w.end, cue.start, cue.end
                    ),
                ));
            }
            if j > 0 && w.start < cue.words[j - 1].end - 1e-9 {
                return Err(CaptionParseError::at(
                    cue.line,
                    format!("cue {no} word {j} {:?} overlaps the previous word", w.text),
                ));
            }
        }
    }
    // Cue overlap refuses naming both cue indexes (REC-002): cues
    // merely touching (`end == start`) never overlap.
    let mut order: Vec<usize> = (0..cues.len()).collect();
    order.sort_by(|&a, &b| {
        cues[a]
            .start
            .total_cmp(&cues[b].start)
            .then_with(|| cues[a].end.total_cmp(&cues[b].end))
            .then_with(|| a.cmp(&b))
    });
    for pair in order.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if cues[b].start < cues[a].end - 1e-9 {
            return Err(CaptionParseError::body(format!(
                "cue {} [{:.3}s, {:.3}s) overlaps cue {} [{:.3}s, {:.3}s)",
                a + 1,
                cues[a].start,
                cues[a].end,
                b + 1,
                cues[b].start,
                cues[b].end
            )));
        }
    }
    Ok(())
}

/// Decode one `.srt` body into cues with uniform word times.
pub fn parse_srt(text: &str) -> Result<DecodedTrack, CaptionParseError> {
    let mut cues = Vec::new();
    let mut notes = Vec::new();
    // Blocks split on blank lines; a block holds the cue number, the
    // timing line, and one or more text lines.
    let mut blocks: Vec<(usize, Vec<(usize, &str)>)> = Vec::new();
    let mut current: Vec<(usize, &str)> = Vec::new();
    for (idx, raw) in text.lines().enumerate() {
        let line_no = idx + 1;
        if raw.trim().is_empty() {
            if !current.is_empty() {
                blocks.push((line_no, std::mem::take(&mut current)));
            }
        } else {
            current.push((line_no, raw));
        }
    }
    if !current.is_empty() {
        blocks.push((text.lines().count() + 1, current));
    }
    if blocks.is_empty() {
        // A cue-less body decodes to the empty track (which renders
        // nothing and re-exports to the same empty file), never to
        // an error: emptiness is not corruption.
        return Ok(DecodedTrack {
            cues: Vec::new(),
            notes: vec!["the file holds no cues and renders nothing".to_string()],
        });
    }
    for (block_no, (_, block)) in blocks.iter().enumerate() {
        let cue_no = block_no + 1;
        if block.len() < 2 {
            return Err(CaptionParseError::at(
                block[0].0,
                format!("cue {cue_no} needs a timing line"),
            ));
        }
        // The first line is the cue number when numeric, else the
        // timing line itself (both shapes decode, never guessed
        // past the arrow).
        let timing_idx = if block[0].1.trim().parse::<u64>().is_ok() {
            if block.len() < 3 {
                return Err(CaptionParseError::at(
                    block[0].0,
                    format!("cue {cue_no} needs a timing line and text"),
                ));
            }
            1
        } else {
            0
        };
        let timing_line = block[timing_idx].0;
        let timing = block[timing_idx].1;
        let (left, right) = timing.split_once("-->").ok_or_else(|| {
            CaptionParseError::at(
                timing_line,
                format!("cue {cue_no} timing line {timing:?} names no `-->` range"),
            )
        })?;
        // A trailing cue-settings field (e.g. `position:50%`) rides
        // after the end time and is ignored, never timed.
        let end_raw = right.trim().split_whitespace().next().unwrap_or("");
        let start = parse_timestamp(left).ok_or_else(|| {
            CaptionParseError::at(
                timing_line,
                format!("cue {cue_no} start time {left:?} does not parse"),
            )
        })?;
        let end = parse_timestamp(end_raw).ok_or_else(|| {
            CaptionParseError::at(
                timing_line,
                format!("cue {cue_no} end time {end_raw:?} does not parse"),
            )
        })?;
        let text = block[timing_idx + 1..]
            .iter()
            .map(|(_, l)| l.trim())
            .collect::<Vec<_>>()
            .join(" ");
        let start = quantize_ms(start);
        let end = quantize_ms(end);
        if end <= start {
            return Err(CaptionParseError::at(
                timing_line,
                format!("cue {cue_no} ends at {end:.3}s, not past its {start:.3}s start"),
            ));
        }
        let words = uniform_words(&text, start, end, cue_no, &mut notes);
        if words.is_empty() && !text.is_empty() {
            notes.push(format!("cue {cue_no} keeps no words and renders nothing"));
        }
        cues.push(CaptionCue {
            start,
            end,
            words,
            line: timing_line,
            column: 1,
        });
    }
    check_cues(&cues)?;
    Ok(DecodedTrack { cues, notes })
}

/// Decode one `.vtt` body (`WEBVTT` head, optional cue identifiers,
/// `NOTE` blocks skipped) into cues with uniform word times.
pub fn parse_vtt(text: &str) -> Result<DecodedTrack, CaptionParseError> {
    let mut lines = text.lines().enumerate().peekable();
    let (first_no, first) = lines
        .next()
        .ok_or_else(|| CaptionParseError::body("empty file holds no cues"))?;
    if first.trim() != "WEBVTT" && !first.trim().starts_with("WEBVTT ") {
        return Err(CaptionParseError::at(
            first_no + 1,
            "missing the `WEBVTT` header",
        ));
    }
    let mut cues = Vec::new();
    let mut notes = Vec::new();
    let mut cue_no = 0usize;
    let mut pending: Vec<(usize, String)> = Vec::new();
    let flush = |pending: &mut Vec<(usize, String)>,
                 cues: &mut Vec<CaptionCue>,
                 notes: &mut Vec<String>,
                 cue_no: &mut usize|
     -> Result<(), CaptionParseError> {
        // Skip blanks and NOTE blocks; an optional identifier line
        // precedes the timing line.
        let mut iter = pending.drain(..).filter(|(_, l)| !l.trim().is_empty());
        let mut first = match iter.next() {
            Some(row) => row,
            None => return Ok(()),
        };
        if first.1.trim() == "NOTE" || first.1.trim().starts_with("NOTE ") {
            return Ok(());
        }
        if !first.1.contains("-->") {
            first = match iter.next() {
                Some(row) => row,
                None => {
                    return Err(CaptionParseError::at(
                        first.0,
                        "cue names no `-->` timing line",
                    ));
                }
            };
            if !first.1.contains("-->") {
                return Err(CaptionParseError::at(
                    first.0,
                    format!("cue timing line {:?} names no `-->` range", first.1),
                ));
            }
        }
        *cue_no += 1;
        let no = *cue_no;
        let (left, right) = first.1.split_once("-->").ok_or_else(|| {
            CaptionParseError::at(
                first.0,
                format!("cue {no} timing line {:?} names no `-->` range", first.1),
            )
        })?;
        let end_raw = right.trim().split_whitespace().next().unwrap_or("");
        let start = parse_timestamp(left).ok_or_else(|| {
            CaptionParseError::at(
                first.0,
                format!("cue {no} start time {:?} does not parse", left.trim()),
            )
        })?;
        let end = parse_timestamp(end_raw).ok_or_else(|| {
            CaptionParseError::at(
                first.0,
                format!("cue {no} end time {end_raw:?} does not parse"),
            )
        })?;
        let text = iter
            .map(|(_, l)| l.trim().to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let start = quantize_ms(start);
        let end = quantize_ms(end);
        if end <= start {
            return Err(CaptionParseError::at(
                first.0,
                format!("cue {no} ends at {end:.3}s, not past its {start:.3}s start"),
            ));
        }
        let words = uniform_words(&text, start, end, no, notes);
        if words.is_empty() && !text.is_empty() {
            notes.push(format!("cue {no} keeps no words and renders nothing"));
        }
        cues.push(CaptionCue {
            start,
            end,
            words,
            line: first.0,
            column: 1,
        });
        Ok(())
    };
    for (idx, raw) in lines {
        if raw.trim().is_empty() {
            flush(&mut pending, &mut cues, &mut notes, &mut cue_no)?;
        } else {
            pending.push((idx + 1, raw.to_string()));
        }
    }
    flush(&mut pending, &mut cues, &mut notes, &mut cue_no)?;
    if cues.is_empty() {
        // A cue-less body decodes to the empty track (which renders
        // nothing and re-exports to the same empty file), never to
        // an error: emptiness is not corruption.
        notes.push("the file holds no cues and renders nothing".to_string());
    }
    check_cues(&cues)?;
    Ok(DecodedTrack { cues, notes })
}

/// One word time read from transcript JSON: every accepted shape
/// carries text plus start/end seconds.
fn json_word(obj: &serde_json::Map<String, serde_json::Value>) -> Option<(String, f64, f64)> {
    let text = obj
        .get("text")
        .or_else(|| obj.get("word"))?
        .as_str()?
        .to_string();
    let start = obj.get("start")?.as_f64()?;
    let end = obj.get("end")?.as_f64()?;
    Some((text, start, end))
}

/// Decode transcript JSON into cues. Two shapes decode: the
/// framesmith words format (`{"cues": [{"start", "end", "words":
/// [{"text", "start", "end"}]}]}`) and whisper/OpenAI-shaped output
/// (`{"segments": [{"start", "end", "text", "words": [{"word"/"text",
/// "start", "end"}]}]}`). Segment words map onto the cue; a segment
/// without word times spreads its text uniformly.
pub fn parse_transcript_json(text: &str) -> Result<DecodedTrack, CaptionParseError> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| CaptionParseError::body(format!("not JSON ({e})")))?;
    let obj = value
        .as_object()
        .ok_or_else(|| CaptionParseError::body("transcript JSON must hold an object"))?;
    let mut cues = Vec::new();
    let mut notes = Vec::new();
    if let Some(list) = obj.get("cues").and_then(|v| v.as_array()) {
        for (i, entry) in list.iter().enumerate() {
            let no = i + 1;
            let cue = entry
                .as_object()
                .ok_or_else(|| CaptionParseError::body(format!("cue {no} must hold an object")))?;
            let num = |key: &str| -> Result<f64, CaptionParseError> {
                cue.get(key).and_then(|v| v.as_f64()).ok_or_else(|| {
                    CaptionParseError::body(format!("cue {no} needs a numeric `{key}`"))
                })
            };
            let start = quantize_ms(num("start")?);
            let end = quantize_ms(num("end")?);
            if end <= start {
                return Err(CaptionParseError::body(format!(
                    "cue {no} ends at {end:.3}s, not past its {start:.3}s start"
                )));
            }
            let words_json = cue
                .get("words")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let mut words = Vec::new();
            for (j, w) in words_json.iter().enumerate() {
                let wobj = w.as_object().ok_or_else(|| {
                    CaptionParseError::body(format!("cue {no} word {j} must hold an object"))
                })?;
                let (wtext, ws, we) = json_word(wobj).ok_or_else(|| {
                    CaptionParseError::body(format!(
                        "cue {no} word {j} needs `text`, `start`, and `end`"
                    ))
                })?;
                if wtext.trim().is_empty() {
                    notes.push(format!("cue {no} word {j} is blank and was dropped"));
                    continue;
                }
                words.push(CaptionWord {
                    text: wtext.trim().to_string(),
                    start: quantize_ms(ws),
                    end: quantize_ms(we),
                });
            }
            if words.is_empty() {
                if let Some(fallback) = cue.get("text").and_then(|v| v.as_str()) {
                    words = uniform_words(fallback, start, end, no, &mut notes);
                }
                if words.is_empty() {
                    notes.push(format!("cue {no} keeps no words and renders nothing"));
                }
            }
            cues.push(CaptionCue {
                start,
                end,
                words,
                line: 0,
                column: 1,
            });
        }
    } else if let Some(list) = obj.get("segments").and_then(|v| v.as_array()) {
        for (i, entry) in list.iter().enumerate() {
            let no = i + 1;
            let seg = entry.as_object().ok_or_else(|| {
                CaptionParseError::body(format!("segment {no} must hold an object"))
            })?;
            let num = |key: &str| -> Result<f64, CaptionParseError> {
                seg.get(key).and_then(|v| v.as_f64()).ok_or_else(|| {
                    CaptionParseError::body(format!("segment {no} needs a numeric `{key}`"))
                })
            };
            let start = quantize_ms(num("start")?);
            let end = quantize_ms(num("end")?);
            if end <= start {
                return Err(CaptionParseError::body(format!(
                    "segment {no} ends at {end:.3}s, not past its {start:.3}s start"
                )));
            }
            let mut words = Vec::new();
            if let Some(arr) = seg.get("words").and_then(|v| v.as_array()) {
                for (j, w) in arr.iter().enumerate() {
                    let wobj = w.as_object().ok_or_else(|| {
                        CaptionParseError::body(format!(
                            "segment {no} word {j} must hold an object"
                        ))
                    })?;
                    let (wtext, ws, we) = json_word(wobj).ok_or_else(|| {
                        CaptionParseError::body(format!(
                            "segment {no} word {j} needs `word`, `start`, and `end`"
                        ))
                    })?;
                    if wtext.trim().is_empty() {
                        notes.push(format!("cue {no} word {j} is blank and was dropped"));
                        continue;
                    }
                    words.push(CaptionWord {
                        text: wtext.trim().to_string(),
                        start: quantize_ms(ws),
                        end: quantize_ms(we),
                    });
                }
            }
            if words.is_empty() {
                let fallback = seg.get("text").and_then(|v| v.as_str()).unwrap_or("");
                words = uniform_words(fallback, start, end, no, &mut notes);
                if words.is_empty() {
                    notes.push(format!("cue {no} keeps no words and renders nothing"));
                }
            }
            cues.push(CaptionCue {
                start,
                end,
                words,
                line: 0,
                column: 1,
            });
        }
    } else {
        return Err(CaptionParseError::body(
            "transcript JSON needs a `cues` or `segments` array",
        ));
    }
    if cues.is_empty() {
        // A cue-less body decodes to the empty track (which renders
        // nothing and re-exports to the same empty file), never to
        // an error: emptiness is not corruption.
        notes.push("the file holds no cues and renders nothing".to_string());
    }
    check_cues(&cues)?;
    Ok(DecodedTrack { cues, notes })
}

/// Decode a caption file body by extension: `.srt`, `.vtt`, or
/// transcript `.json`. Any other extension refuses naming the file.
pub fn decode_named(name: &str, body: &str) -> Result<DecodedTrack, CaptionParseError> {
    let lower = name.to_lowercase();
    if lower.ends_with(".srt") {
        parse_srt(body)
    } else if lower.ends_with(".vtt") {
        parse_vtt(body)
    } else if lower.ends_with(".json") {
        parse_transcript_json(body)
    } else {
        Err(CaptionParseError::body(format!(
            "file {name:?} names no caption format (`.srt`, `.vtt`, or transcript `.json`)"
        )))
    }
}

/// Export cues to SRT (`HH:MM:SS,mmm`): exactly what the track
/// holds, no re-timing, so re-import drifts by zero.
pub fn format_srt(cues: &[CaptionCue]) -> String {
    let mut out = String::new();
    for (i, cue) in cues.iter().enumerate() {
        out.push_str(&format!("{}\n", i + 1));
        out.push_str(&format!(
            "{} --> {}\n",
            format_timestamp(cue.start, true),
            format_timestamp(cue.end, true)
        ));
        let text = cue
            .words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        out.push_str(&text);
        out.push_str("\n\n");
    }
    out
}

/// Export cues to VTT (`WEBVTT` head, `HH:MM:SS.mmm`): exactly what
/// the track holds, no re-timing, so re-import drifts by zero.
pub fn format_vtt(cues: &[CaptionCue]) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for cue in cues {
        out.push_str(&format!(
            "{} --> {}\n",
            format_timestamp(cue.start, false),
            format_timestamp(cue.end, false)
        ));
        let text = cue
            .words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        out.push_str(&text);
        out.push_str("\n\n");
    }
    out
}

/// Export cues to the framesmith words JSON: cue and word times ride
/// as seconds, exactly what the track holds.
pub fn format_transcript_json(cues: &[CaptionCue]) -> String {
    let mut out = String::from("{\"cues\":[");
    for (i, cue) in cues.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"start\":{:.3},\"end\":{:.3},\"words\":[",
            cue.start, cue.end
        ));
        for (j, w) in cue.words.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"text\":{},\"start\":{:.3},\"end\":{:.3}}}",
                json_escape(&w.text),
                w.start,
                w.end
            ));
        }
        out.push_str("]}");
    }
    out.push_str("]}");
    out
}

/// Minimal JSON string escaper for caption word text.
fn json_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The active cue at clip-local time `t` (half-open `start <= t <
/// end`): the cue whose words render at this instant. `None` shows
/// no words.
pub fn active_cue(cues: &[CaptionCue], t: f64) -> Option<usize> {
    if !t.is_finite() {
        return None;
    }
    cues.iter().position(|cue| t >= cue.start && t < cue.end)
}

/// The active word inside `cue` at clip-local time `t` (half-open):
/// the spoken word the highlight presets address. `None` while no
/// word holds this instant.
pub fn active_word(cue: &CaptionCue, t: f64) -> Option<usize> {
    if !t.is_finite() {
        return None;
    }
    cue.words.iter().position(|w| t >= w.start && t < w.end)
}

/// Entrance preset families (Sprint 22 captions, T002 / CAP-004):
/// 13 pure functions of film time. Each name below is accepted on
/// the `entrance` key; anything else refuses naming the preset.
pub const ENTRANCE_PRESETS: [&str; 13] = [
    "none",
    "fade-in",
    "fade-up",
    "fade-down",
    "fade-left",
    "fade-right",
    "fade-zoom-in",
    "fade-zoom-out",
    "pop",
    "slide-up",
    "slide-down",
    "blur-in",
    "typewriter",
];

/// Highlight preset families (Sprint 22 captions, T002 / CAP-004):
/// 7 pure functions of film time addressing the spoken word.
pub const HIGHLIGHT_PRESETS: [&str; 7] = [
    "none",
    "pop",
    "fade",
    "dim-others",
    "underline",
    "background",
    "glow",
];

/// Exit preset families (Sprint 22 captions, T002 / CAP-004): 11
/// pure functions of film time.
pub const EXIT_PRESETS: [&str; 11] = [
    "none",
    "fade-out",
    "fade-up-out",
    "fade-down-out",
    "slide-left-out",
    "slide-right-out",
    "zoom-out",
    "shrink",
    "blur-out",
    "drop",
    "typewriter-out",
];

/// True for a known entrance preset name.
pub fn is_entrance(name: &str) -> bool {
    ENTRANCE_PRESETS.contains(&name)
}

/// True for a known highlight preset name.
pub fn is_highlight(name: &str) -> bool {
    HIGHLIGHT_PRESETS.contains(&name)
}

/// True for a known exit preset name.
pub fn is_exit(name: &str) -> bool {
    EXIT_PRESETS.contains(&name)
}

/// Entrance window in seconds: the first 0.3s of the cue, or half
/// the cue when shorter. Presets ease over this window only.
pub fn entrance_window(cue: &CaptionCue) -> f64 {
    (0.3f64).min((cue.end - cue.start) / 2.0).max(0.0)
}

/// Exit window in seconds: the last 0.3s of the cue, or half the cue
/// when shorter.
pub fn exit_window(cue: &CaptionCue) -> f64 {
    entrance_window(cue)
}

/// Smoothstep easing shared by every preset curve: pure film-time
/// math, no wall clock, no randomness.
pub fn smooth(p: f64) -> f64 {
    let c = p.clamp(0.0, 1.0);
    c * c * (3.0 - 2.0 * c)
}

/// One entrance sample (Sprint 22 captions, T002): the alpha, pixel
/// offset, and scale the entrance preset applies at film time `t`
/// (clip-local). `(1, 0, 0, 1)` past the window means the preset
/// holds the resting words, so reruns agree byte for byte.
pub fn entrance_state(name: &str, cue: &CaptionCue, t: f64) -> (f64, f64, f64, f64) {
    if name == "none" {
        return (1.0, 0.0, 0.0, 1.0);
    }
    let win = entrance_window(cue);
    if win <= 0.0 || t >= cue.start + win {
        return (1.0, 0.0, 0.0, 1.0);
    }
    let p = smooth((t - cue.start).max(0.0) / win);
    // (alpha, dx, dy, scale) at rest each ease from their start.
    match name {
        "fade-in" => (p, 0.0, 0.0, 1.0),
        "fade-up" => (p, 0.0, 12.0 * (1.0 - p), 1.0),
        "fade-down" => (p, 0.0, -12.0 * (1.0 - p), 1.0),
        "fade-left" => (p, 12.0 * (1.0 - p), 0.0, 1.0),
        "fade-right" => (p, -12.0 * (1.0 - p), 0.0, 1.0),
        "fade-zoom-in" => (p, 0.0, 0.0, 0.85 + 0.15 * p),
        "fade-zoom-out" => (p, 0.0, 0.0, 1.15 - 0.15 * p),
        "pop" => (p.min(1.0), 0.0, 0.0, 0.6 + 0.4 * smooth((p * 1.4).min(1.0))),
        "slide-up" => (1.0, 0.0, 48.0 * (1.0 - p), 1.0),
        "slide-down" => (1.0, 0.0, -48.0 * (1.0 - p), 1.0),
        "blur-in" => (p, 0.0, 0.0, 1.04 - 0.04 * p),
        "typewriter" => (p, -6.0 * (1.0 - p), 0.0, 1.0),
        _ => (1.0, 0.0, 0.0, 1.0),
    }
}

/// Extra vertical smear passes the `blur-in` entrance (and `blur-out`
/// exit) draws while resolving: `[(dy, alpha_scale)]`, empty for
/// sharp presets. The smear reads as defocus resolving, computed
/// purely from film time.
pub fn blur_passes(name: &str, cue: &CaptionCue, t: f64, exiting: bool) -> Vec<(f64, f64)> {
    let blurring = (!exiting && name == "blur-in") || (exiting && name == "blur-out");
    if !blurring {
        return Vec::new();
    }
    let (win, edge) = if exiting {
        (exit_window(cue), cue.end - t)
    } else {
        (entrance_window(cue), t - cue.start)
    };
    if win <= 0.0 {
        return Vec::new();
    }
    let p = smooth(edge.max(0.0) / win);
    if p >= 1.0 {
        return Vec::new();
    }
    let spread = 2.0 * (1.0 - p);
    vec![(-spread, 0.3), (spread, 0.3)]
}

/// One exit sample (Sprint 22 captions, T002): the alpha, pixel
/// offset, and scale the exit preset applies at film time `t`
/// (clip-local). `(1, 0, 0, 1)` before the window means the words
/// rest until the exit begins.
pub fn exit_state(name: &str, cue: &CaptionCue, t: f64) -> (f64, f64, f64, f64) {
    if name == "none" {
        return (1.0, 0.0, 0.0, 1.0);
    }
    let win = exit_window(cue);
    if win <= 0.0 || t < cue.end - win {
        return (1.0, 0.0, 0.0, 1.0);
    }
    let p = smooth((t - (cue.end - win)).max(0.0) / win);
    match name {
        "fade-out" => (1.0 - p, 0.0, 0.0, 1.0),
        "fade-up-out" => (1.0 - p, 0.0, -12.0 * p, 1.0),
        "fade-down-out" => (1.0 - p, 0.0, 12.0 * p, 1.0),
        "slide-left-out" => (1.0, -48.0 * p, 0.0, 1.0),
        "slide-right-out" => (1.0, 48.0 * p, 0.0, 1.0),
        "zoom-out" => (1.0 - p, 0.0, 0.0, 1.0 + 0.15 * p),
        "shrink" => (1.0 - p, 0.0, 0.0, 1.0 - 0.4 * p),
        "blur-out" => (1.0 - p, 0.0, 0.0, 1.0 + 0.04 * p),
        "drop" => (1.0 - p * p, 0.0, 60.0 * p * p, 1.0),
        "typewriter-out" => (1.0 - p, 6.0 * p, 0.0, 1.0),
        _ => (1.0, 0.0, 0.0, 1.0),
    }
}

/// One highlight sample (Sprint 22 captions, T002): the scale and
/// opacity factors plus the marker (`None`, `Underline`,
/// `Background`, or `Glow`) the highlight preset applies to one
/// word. `spoken` is true for the word holding the film time, and
/// `past` for words it already passed. Pure film-time math.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightMark {
    None,
    Underline,
    Background,
    Glow,
}

pub fn highlight_state(name: &str, spoken: bool, past: bool) -> (f64, f64, HighlightMark) {
    match name {
        "none" | "" => (1.0, 1.0, HighlightMark::None),
        "pop" => (if spoken { 1.25 } else { 1.0 }, 1.0, HighlightMark::None),
        "fade" => (
            1.0,
            if spoken {
                1.0
            } else if past {
                0.75
            } else {
                0.45
            },
            HighlightMark::None,
        ),
        "dim-others" => (1.0, if spoken { 1.0 } else { 0.35 }, HighlightMark::None),
        "underline" => (
            1.0,
            1.0,
            if spoken {
                HighlightMark::Underline
            } else {
                HighlightMark::None
            },
        ),
        "background" => (
            1.0,
            1.0,
            if spoken {
                HighlightMark::Background
            } else {
                HighlightMark::None
            },
        ),
        "glow" => (
            1.0,
            1.0,
            if spoken {
                HighlightMark::Glow
            } else {
                HighlightMark::None
            },
        ),
        _ => (1.0, 1.0, HighlightMark::None),
    }
}
