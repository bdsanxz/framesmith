//! Background-removal runner (Sprint 23, T004 / CAP-004).
//!
//! `remove-background <input>` emits a subject-alpha PNG plus an
//! inverse background plate that drops into the Sprint 22
//! subject-matte slot. Quality presets only change the model
//! selected. CPU only, one clip per invocation; still pictures only —
//! anything else refuses naming the input.

use crate::catalog::QUALITY_MODELS;
use crate::config::MlConfig;
use crate::engines::{resolve_engine, run_engine_process, ResolvedEngine};
use crate::models::require_model;

/// Accepted quality presets (each selects its segment model).
pub const QUALITIES: &[&str] = &["fast", "balanced", "precise"];

/// Model for a quality preset; unknown presets refuse naming the
/// preset and every accepted one.
pub fn model_for_quality(quality: &str) -> Result<&'static str, (String, String)> {
    QUALITY_MODELS
        .iter()
        .find(|(q, _)| *q == quality)
        .map(|(_, m)| *m)
        .ok_or_else(|| {
            (
                "E_BAD_MATTE".to_string(),
                format!(
                    "remove-background refused (E_BAD_MATTE): unknown `--quality {quality:?}`.\n\
                     fix: use one of {}. No work was done.",
                    QUALITIES.join(", ")
                ),
            )
        })
}

/// Run background removal. On success returns `(alpha_path,
/// plate_path)`. Every failure names the file or piece and the
/// cause; no partial artifact is ever staged.
#[allow(clippy::too_many_arguments)]
pub fn run_remove_background(
    cfg: &MlConfig,
    input: &str,
    quality: &str,
    model_override: Option<&str>,
    engine: &str,
    engine_bin: Option<&str>,
    out_dir: Option<&str>,
) -> Result<(String, String), (String, String)> {
    let model_name = match model_override {
        Some(m) => m.to_string(),
        None => model_for_quality(quality)?.to_string(),
    };
    let Some(model) = crate::catalog::find_model(&model_name) else {
        return Err((
            "E_NO_MODEL".to_string(),
            format!(
                "remove-background refused (E_NO_MODEL): unknown model {model_name:?}.\n\
                 fix: use one of rembg-fast, rembg-base, rembg-precise. No work was done.",
            ),
        ));
    };
    if model.kind != "segment" {
        return Err((
            "E_NO_MODEL".to_string(),
            format!(
                "remove-background refused (E_NO_MODEL): model {} serves `{}` runs, not segmentation.\n\
                 fix: use rembg-fast, rembg-base, or rembg-precise. No work was done.",
                model.name, model.kind
            ),
        ));
    }
    let in_path = std::path::Path::new(input);
    if !in_path.is_file() {
        return Err((
            "E_BAD_MATTE".to_string(),
            format!(
                "remove-background refused (E_BAD_MATTE): input {input:?} cannot be read.\n\
                 fix: pass an existing still picture (PNG). No work was done and no partial artifact was staged."
            ),
        ));
    }
    if !input.to_lowercase().ends_with(".png") {
        return Err((
            "E_BAD_MATTE".to_string(),
            format!(
                "remove-background refused (E_BAD_MATTE): input {input:?} is not a still PNG picture.\n\
                 fix: pass a PNG still; video and other formats are not supported by this runner. No work was done and no partial artifact was staged."
            ),
        ));
    }
    let engine = resolve_engine("segment", engine, engine_bin)?;
    let model_path = require_model(cfg, model.name)?;
    let dir = out_dir.map(|s| s.to_string()).unwrap_or_else(|| {
        let stem = in_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "matte".to_string());
        format!("{stem}.bg")
    });
    std::fs::create_dir_all(&dir).map_err(|e| {
        (
            "E_BAD_MATTE".to_string(),
            format!(
                "remove-background refused (E_BAD_MATTE): cannot create output dir {dir:?}: {e}. No artifact was staged."
            ),
        )
    })?;
    let alpha_path = format!("{dir}/subject-alpha.png");
    let plate_path = format!("{dir}/background-plate.png");
    // Engine contract: `<bin> --model --input --alpha-out
    // --plate-out` writes both PNGs itself. The stub proves the same
    // contract through the hidden re-exec verb.
    let args = vec![
        "--model".to_string(),
        model_path.display().to_string(),
        "--input".to_string(),
        input.to_string(),
        "--alpha-out".to_string(),
        alpha_path.clone(),
        "--plate-out".to_string(),
        plate_path.clone(),
    ];
    let map_err = |cause: String| {
        (
            "E_ENGINE_FAILED".to_string(),
            format!(
                "remove-background refused (E_ENGINE_FAILED): {cause}. No artifact was staged."
            ),
        )
    };
    match &engine {
        ResolvedEngine::Stub { .. } => {
            crate::stub::spawn_stub("segment", &args).map_err(map_err)?;
        }
        ResolvedEngine::Explicit { bin, .. } | ResolvedEngine::Host { bin, .. } => {
            run_engine_process(bin, &args, cfg.download_timeout_secs).map_err(map_err)?;
        }
    }
    // Both PNGs on disk are authoritative: each must decode as a
    // still picture (the same bar the Sprint 22 matte slot applies),
    // or the outputs are corrupt and both are discarded.
    for path in [&alpha_path, &plate_path] {
        if let Err(cause) = decode_still_png(path) {
            let _ = std::fs::remove_file(&alpha_path);
            let _ = std::fs::remove_file(&plate_path);
            return Err((
                "E_ENGINE_FAILED".to_string(),
                format!(
                    "remove-background refused (E_ENGINE_FAILED): engine `{}` wrote an unreadable PNG {path:?} ({cause}). No artifact was staged.",
                    engine.name()
                ),
            ));
        }
    }
    Ok((alpha_path, plate_path))
}

/// Decode-check one still PNG: readable, non-animated. Mirrors the
/// Sprint 22 matte bar (`matte_png_decodes`).
pub fn decode_still_png(path: &str) -> Result<(u32, u32, Vec<u8>), String> {
    let file = std::fs::File::open(path).map_err(|e| format!("cannot be read: {e}"))?;
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("is not a readable PNG picture ({e})"))?;
    if reader.info().is_animated() {
        return Err("is an animated PNG; mattes are still pictures".to_string());
    }
    let (w, h) = (reader.info().width, reader.info().height);
    let mut buf = vec![0u8; reader.output_buffer_size()];
    reader
        .next_frame(&mut buf)
        .map_err(|e| format!("cannot be decoded ({e})"))?;
    Ok((w, h, buf))
}

/// Stub segmentation as files: decode the still, build the center
/// subject mask, write the alpha PNG (white RGBA under the mask) and
/// the plate PNG (source where the mask is transparent, mean
/// background fill where it is not). A pure function of the input
/// bytes, so reruns are byte-identical.
pub fn stub_segment_files(input: &str, alpha_out: &str, plate_out: &str) -> Result<(), String> {
    let (w, h, rgba) =
        decode_still_png(input).map_err(|cause| format!("input {input:?} {cause}"))?;
    if rgba.len() != (w * h * 4) as usize {
        return Err(format!(
            "input {input:?} decoded to an unexpected pixel layout"
        ));
    }
    let alpha = crate::stub::stub_alpha(w, h);
    // Mean background color from fully transparent pixels; mid gray
    // when the mask covers everything.
    let (mut sr, mut sg, mut sb, mut n) = (0u64, 0u64, 0u64, 0u64);
    for (i, a) in alpha.iter().enumerate() {
        if *a == 0 {
            sr += rgba[i * 4] as u64;
            sg += rgba[i * 4 + 1] as u64;
            sb += rgba[i * 4 + 2] as u64;
            n += 1;
        }
    }
    let (fr, fg, fb) = if n == 0 {
        (128, 128, 128)
    } else {
        let avg = |sum: u64| (sum / n) as u8;
        (avg(sr), avg(sg), avg(sb))
    };
    let mut alpha_px = vec![0u8; (w * h * 4) as usize];
    let mut plate_px = vec![0u8; (w * h * 3) as usize];
    for (i, a) in alpha.iter().enumerate() {
        alpha_px[i * 4] = 255;
        alpha_px[i * 4 + 1] = 255;
        alpha_px[i * 4 + 2] = 255;
        alpha_px[i * 4 + 3] = *a;
        if *a == 0 {
            plate_px[i * 3] = rgba[i * 4];
            plate_px[i * 3 + 1] = rgba[i * 4 + 1];
            plate_px[i * 3 + 2] = rgba[i * 4 + 2];
        } else {
            plate_px[i * 3] = fr;
            plate_px[i * 3 + 1] = fg;
            plate_px[i * 3 + 2] = fb;
        }
    }
    write_png_rgba(alpha_out, w, h, &alpha_px)?;
    write_png_rgb(plate_out, w, h, &plate_px)?;
    return Ok(());

    fn write_png_rgba(path: &str, w: u32, h: u32, px: &[u8]) -> Result<(), String> {
        write_png(path, w, h, png::ColorType::Rgba, px)
    }

    fn write_png_rgb(path: &str, w: u32, h: u32, px: &[u8]) -> Result<(), String> {
        write_png(path, w, h, png::ColorType::Rgb, px)
    }

    fn write_png(
        path: &str,
        w: u32,
        h: u32,
        color: png::ColorType,
        px: &[u8],
    ) -> Result<(), String> {
        if let Some(parent) = std::path::Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("cannot create output dir {}: {e}", parent.display()))?;
            }
        }
        let file =
            std::fs::File::create(path).map_err(|e| format!("cannot write {path:?}: {e}"))?;
        let mut enc = png::Encoder::new(file, w, h);
        enc.set_color(color);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc
            .write_header()
            .map_err(|e| format!("cannot write {path:?}: {e}"))?;
        writer
            .write_image_data(px)
            .map_err(|e| format!("cannot write {path:?}: {e}"))
    }
}
