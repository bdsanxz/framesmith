//! The font collection of one composition: the fonts the project
//! declares, plus faces of the machine that join when a text names
//! their family or the fallback takes them.
//!
//! Registration and family resolution follow the old engine, with the
//! shaping backend swapped: bytes register into parley's `fontique`
//! collection (no engine type leaks past this crate), coverage reads
//! use the std-only `glyph::tables` readers instead of `read-fonts`, and
//! emoji pictures decode through FFmpeg instead of frx7.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use parley::fontique::{Blob, Collection, CollectionOptions, FontInfoOverride};

use glyph::tables::{has_sbix, maps_char, one_face, sbix_graphic};

use crate::machine::Machine;
use crate::paragraph::TextError;

/// The start of the name under which the engine knows a family of the
/// machine, so that no family of a composition is ever taken for one,
/// or joined to one.
const MACHINE_PREFIX: &str = "machine font: ";

/// Why a file is not registered as a font.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontError {
    /// A font of a kind this version does not draw (WOFF/WOFF2, CFF outlines).
    Unsupported(&'static str),
    /// The bytes are not a font of any kind.
    NotAFont,
    /// A font collection this version does not draw, with the reason.
    Collection(String),
    /// The bytes begin as a font but the reader refuses them, with its reason.
    Damaged(String),
}

/// One face registered in the collection: the engine family it belongs
/// to, the parley blob id and bytes, its index in the file, and — for a
/// face of the machine — its file and index there.
#[derive(Clone)]
pub(crate) struct Face {
    pub(crate) family: String,
    pub(crate) blob: u64,
    pub(crate) bytes: Arc<Vec<u8>>,
    pub(crate) index: u32,
    pub(crate) machine: Option<(PathBuf, u32)>,
}

/// What the layout pass needs of a registered face: owned, so the
/// emoji decode (`&mut`) never fights the run walk (shared).
#[derive(Clone)]
pub(crate) struct FaceInfo {
    pub(crate) family: String,
    pub(crate) bytes: Arc<Vec<u8>>,
    pub(crate) index: u32,
    pub(crate) colour: bool,
}

impl Fonts {
    /// The registered face parley shaped `blob`/`index` with, if it is
    /// one of this collection's faces.
    pub(crate) fn face_for_blob(&self, blob: u64, index: u32) -> Option<FaceInfo> {
        self.faces
            .iter()
            .find(|face| (face.blob, face.index) == (blob, index))
            .map(|face| FaceInfo {
                family: face.family.clone(),
                bytes: face.bytes.clone(),
                index: face.index,
                colour: has_sbix(&face.bytes, face.index),
            })
    }
}

/// A face of the machine that draws in a collection: its file, its
/// index in that file, and its family. The renderer names these in the
/// success line, so the origin is never silent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineFace {
    pub path: PathBuf,
    pub index: u32,
    pub family: String,
}

/// A decoded color-emoji picture, straight RGBA rows without padding,
/// plus its `sbix` origin offsets in design units and the face's units
/// per em for placement.
#[derive(Clone, Debug)]
pub struct EmojiPic {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<Vec<u8>>,
    pub origin_x: i16,
    pub origin_y: i16,
    pub units_per_em: u16,
}

/// The font collection of one composition. It starts with no family:
/// the system's fonts are never read until asked for.
pub struct Fonts {
    pub(crate) context: parley::FontContext,
    pub(crate) layouts: parley::LayoutContext<[u8; 4]>,
    faces: Vec<Face>,
    machine: Machine,
    /// Families of the machine registered so far, by their name on the
    /// machine: the name the engine knows each by, or `None` when none
    /// of its faces could be registered.
    machine_families: HashMap<String, Option<String>>,
    /// The machine family the fallback gives each character asked so far.
    fall_backs: HashMap<char, Option<String>>,
    /// Decoded emoji pictures, by (family, glyph, size bits).
    emoji: HashMap<(String, u32, u32), Arc<EmojiPic>>,
    /// `FRAMESMITH_SYSTEM_FONT` override: the family it registered as,
    /// used for every family the composition does not declare — the way
    /// the old candidate list put the override file first for all styles.
    override_family: Option<String>,
}

impl Fonts {
    /// A collection with no font in it. Honors `FRAMESMITH_SYSTEM_FONT`:
    /// when it names a readable font file, that file replaces the host
    /// candidate list for every family the composition does not declare
    /// (the render tests pin it to a renamed Arial).
    pub fn new() -> Fonts {
        let mut fonts = Fonts {
            context: parley::FontContext {
                collection: Collection::new(CollectionOptions {
                    shared: false,
                    system_fonts: false,
                }),
                source_cache: Default::default(),
            },
            layouts: parley::LayoutContext::new(),
            faces: Vec::new(),
            machine: Machine::scan(),
            machine_families: HashMap::new(),
            fall_backs: HashMap::new(),
            emoji: HashMap::new(),
            override_family: None,
        };
        fonts.register_override();
        fonts
    }

    /// Resolves the system face for a style, registering it under the
    /// file stem (`Arial Bold`): the old candidate-file behavior —
    /// `bold`/`italic` draw the true variant outlines where the machine
    /// carries them, else the regular face with parley synthesis — kept
    /// inside the parley collection so the success line keeps naming the
    /// same basenames. The `FRAMESMITH_SYSTEM_FONT` override wins for
    /// every style, as before.
    pub fn resolve_system_style(&mut self, bold: bool, italic: bool) -> Option<String> {
        if let Some(over) = self.override_family.clone() {
            return Some(over);
        }
        let supplemental = "/System/Library/Fonts/Supplemental";
        let shared = "/System/Library/Fonts";
        let dejavu = "/usr/share/fonts/truetype/dejavu";
        let names: &[(&str, &str)] = match (bold, italic) {
            (false, false) => &[
                ("Arial.ttf", supplemental),
                ("Verdana.ttf", supplemental),
                ("Helvetica.ttc", shared),
                ("DejaVuSans.ttf", dejavu),
            ],
            (true, false) => &[
                ("Arial Bold.ttf", supplemental),
                ("Verdana Bold.ttf", supplemental),
                ("DejaVuSans-Bold.ttf", dejavu),
            ],
            (false, true) => &[
                ("Arial Italic.ttf", supplemental),
                ("Verdana Italic.ttf", supplemental),
                ("DejaVuSans-Oblique.ttf", dejavu),
            ],
            (true, true) => &[
                ("Arial Bold Italic.ttf", supplemental),
                ("Verdana Bold Italic.ttf", supplemental),
                ("DejaVuSans-BoldOblique.ttf", dejavu),
            ],
        };
        for (name, dir) in names {
            let path = PathBuf::from(format!("{dir}/{name}"));
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(name)
                .to_string();
            let plain;
            let bytes: &[u8] = if bytes.starts_with(b"ttcf") {
                let Some(first) = one_face(&bytes, 0) else {
                    continue;
                };
                plain = first;
                &plain
            } else {
                &bytes
            };
            let before = self.faces.len();
            if self.register(bytes.to_vec(), Some(&stem)).is_err() {
                continue;
            }
            for face in &mut self.faces[before..] {
                face.machine = Some((path.clone(), 0));
            }
            return Some(stem);
        }
        // No variant anywhere: the regular face carries the style with
        // parley synthesis, exactly like the old fallback.
        if bold || italic {
            return self.resolve_system_style(false, false);
        }
        None
    }

    /// Registers the override file, if set and readable, as a machine
    /// face under its own family name. An unreadable override is
    /// skipped: the machine scan still applies.
    fn register_override(&mut self) {
        let path = match std::env::var_os("FRAMESMITH_SYSTEM_FONT") {
            Some(path) => PathBuf::from(path),
            None => return,
        };
        if path.as_os_str().is_empty() {
            return;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        let Some(family) = glyph::tables::family_name(&bytes, 0).filter(|name| !name.is_empty())
        else {
            return;
        };
        let before = self.faces.len();
        if self.register(bytes, Some(family.as_str())).is_err() {
            return;
        }
        for face in &mut self.faces[before..] {
            face.machine = Some((path.clone(), 0));
        }
        self.override_family = Some(family);
    }

    /// Registers the bytes of a font file. With `family`, the face is
    /// named by it; without, by the family name the file gives itself.
    /// A collection registers its first face; anything the CPU renderer
    /// cannot draw (WOFF, CFF) refuses with its reason.
    pub fn register(&mut self, bytes: Vec<u8>, family: Option<&str>) -> Result<(), FontError> {
        match bytes.get(..4) {
            Some([0, 1, 0, 0]) | Some(b"true") | Some(b"OTTO") => {}
            Some(b"ttcf") => {}
            Some(b"wOF2") => return Err(FontError::Unsupported("WOFF2")),
            _ if bytes.starts_with(b"wOFF") => return Err(FontError::Unsupported("WOFF")),
            _ => return Err(FontError::NotAFont),
        }
        if let Err(reason) = glyph::font::check_supported(&bytes).map_err(|e| e) {
            if reason.contains("WOFF") {
                return Err(FontError::Unsupported("WOFF/WOFF2"));
            }
            if reason.contains("CFF") {
                return Err(FontError::Unsupported("OpenType with PostScript outlines"));
            }
            if reason.contains("not recognized") {
                return Err(FontError::NotAFont);
            }
            return Err(FontError::Damaged(reason));
        }
        let plain;
        let bytes: &[u8] = if bytes.starts_with(b"ttcf") {
            plain = one_face(&bytes, 0)
                .ok_or_else(|| FontError::Damaged("a collection face cannot be read".into()))?;
            &plain
        } else {
            &bytes
        };
        let name = match family {
            Some(name) => name.to_string(),
            None => glyph::tables::family_name(bytes, 0)
                .ok_or_else(|| FontError::Damaged("a face with no family name".into()))?,
        };
        let bytes = Arc::new(bytes.to_vec());
        let blob = Blob::new(bytes.clone());
        let blob_id = blob.id();
        let info_override = family.map(|_| FontInfoOverride {
            family_name: Some(&name),
            ..Default::default()
        });
        let registered = self.context.collection.register_fonts(blob, info_override);
        if registered.is_empty() {
            return Err(FontError::Damaged(
                "the shaper takes no face from it".into(),
            ));
        }
        self.faces.push(Face {
            family: name,
            blob: blob_id,
            bytes,
            index: 0,
            machine: None,
        });
        Ok(())
    }

    /// The families of a `font-family` list as the engine knows them:
    /// each name a family of the composition, a generic family, or a
    /// family of the machine, in list order with absent names left out.
    /// A list whose names are all absent falls back to the missing-mark
    /// family where the machine has it, else there is no family.
    /// Whether at least one of `names` resolves to a registered project
    /// family or a machine family. Used to fail closed on an explicit
    /// `font-family` that names nothing usable, instead of silently
    /// falling back to the missing-glyph face.
    pub fn resolves_any(&mut self, names: &[String]) -> bool {
        names.iter().any(|name| self.resolve_one(name).is_some())
    }

    /// One requested family name to the engine family to use: a
    /// registered composition family directly, else the machine family
    /// (through the generic-name mapping). `None` when neither has it.
    fn resolve_one(&mut self, name: &str) -> Option<String> {
        if self.faces.iter().any(|face| face.family == *name) {
            // A composition family, a style stem (`Arial Bold`), or
            // the override file: already registered, use directly.
            return Some(name.to_string());
        }
        let candidates: Vec<String> = match Machine::generic(name) {
            Some((_, mac)) => {
                let mut v = vec![mac.to_string()];
                if let Some(linux) = Machine::generic_linux(name) {
                    v.push(linux.to_string());
                }
                if let Some(windows) = Machine::generic_windows(name) {
                    v.push(windows.to_string());
                }
                v
            }
            None => vec![name.to_string()],
        };
        let hit = candidates
            .into_iter()
            .filter_map(|wanted| self.machine.family(&wanted).map(str::to_string))
            .next();
        hit.and_then(|found| self.machine_family(&found))
    }

    pub(crate) fn families_of(&mut self, names: &[String]) -> Result<Vec<String>, TextError> {
        let mut families: Vec<String> = Vec::new();
        for name in names {
            let family = self.resolve_one(name);
            if let Some(family) = family {
                if !families.contains(&family) {
                    families.push(family);
                }
            }
        }
        if families.is_empty() {
            match self.machine.missing_mark_family().map(str::to_string) {
                Some(family) => families.extend(self.machine_family(&family)),
                None => return Err(TextError::NoFamily),
            }
        }
        if families.is_empty() {
            return Err(TextError::NoFamily);
        }
        Ok(families)
    }

    /// Whether a face of one of `families` maps `character` past glyph 0.
    pub(crate) fn families_map(&self, families: &[String], character: char) -> bool {
        self.faces
            .iter()
            .filter(|face| families.contains(&face.family))
            .any(|face| maps_char(&face.bytes, face.index, character))
    }

    /// The machine family the fallback gives `character`: the first
    /// family in fallback order mapping it, registered as the engine
    /// knows it. `None` when no machine family maps it.
    pub(crate) fn fall_back(&mut self, character: char) -> Option<String> {
        let family = match self.fall_backs.get(&character) {
            Some(found) => found.clone(),
            None => {
                let order: Vec<String> = self
                    .machine
                    .fall_back_order()
                    .into_iter()
                    .map(str::to_string)
                    .collect();
                let found = order
                    .into_iter()
                    .find(|family| self.machine.family_maps(family, character));
                self.fall_backs.insert(character, found.clone());
                found
            }
        }?;
        self.machine_family(&family)
    }

    /// Registers the faces of the machine's `family` once, and gives
    /// the name the engine knows it by. Outline faces register alone
    /// under a prefixed name (never taken for a composition family);
    /// color faces register whole so their `sbix` pictures stay readable.
    fn machine_family(&mut self, family: &str) -> Option<String> {
        if let Some(known) = self.machine_families.get(family) {
            return known.clone();
        }
        let engine_name = format!("{MACHINE_PREFIX}{family}");
        let mut registered = None;
        for at in self.machine.faces_of(family) {
            let face = &self.machine.faces[at];
            let bytes = if face.colour {
                std::fs::read(&face.path).ok()?
            } else {
                let raw = std::fs::read(&face.path).ok()?;
                one_face(&raw, face.index).unwrap_or(raw)
            };
            let bytes = Arc::new(bytes);
            let blob = Blob::new(bytes.clone());
            let blob_id = blob.id();
            let infos = self.context.collection.register_fonts(
                blob,
                Some(FontInfoOverride {
                    family_name: Some(&engine_name),
                    ..Default::default()
                }),
            );
            if infos.is_empty() {
                continue;
            }
            let index = if face.colour { face.index } else { 0 };
            self.faces.push(Face {
                family: engine_name.clone(),
                blob: blob_id,
                bytes,
                index,
                machine: Some((face.path.clone(), face.index)),
            });
            registered = Some(engine_name.clone());
        }
        self.machine_families
            .insert(family.to_string(), registered.clone());
        registered
    }

    /// The faces of the machine registered so far, in registration order.
    pub fn machine_faces(&self) -> Vec<MachineFace> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for face in &self.faces {
            if let Some((path, index)) = face.machine.clone() {
                if seen.insert((path.clone(), index)) {
                    let family = self
                        .machine
                        .faces
                        .iter()
                        .find(|f| f.path == path && f.index == index)
                        .map(|f| f.family.clone())
                        .unwrap_or_default();
                    out.push(MachineFace {
                        path,
                        index,
                        family,
                    });
                }
            }
        }
        out
    }

    /// The number of registered faces, of every family.
    pub fn len(&self) -> usize {
        self.faces.len()
    }

    /// Whether no face is registered.
    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    /// The decoded emoji picture of `ch` at `size_px`: the registered
    /// face holding its `sbix` graphic, decoded by `ffmpeg_bin` (the
    /// frx7 replacement) and cached for the life of the collection.
    pub(crate) fn emoji_picture(
        &mut self,
        ffmpeg_bin: &std::path::Path,
        ch: char,
        size_px: f32,
    ) -> Result<Arc<EmojiPic>, String> {
        let mut found: Option<(String, u32, Vec<u8>, i16, i16, u16, u16)> = None;
        for face in &self.faces {
            if !has_sbix(&face.bytes, face.index) {
                continue;
            }
            let Some(gid) = glyph::tables::glyph_id(&face.bytes, face.index, ch) else {
                continue;
            };
            let Some(graphic) = sbix_graphic(&face.bytes, face.index, gid as usize, size_px) else {
                continue;
            };
            if graphic.kind != *b"png " {
                continue;
            }
            let Some(upm) = glyph::tables::units_per_em(&face.bytes, face.index) else {
                continue;
            };
            found = Some((
                face.family.clone(),
                gid as u32,
                graphic.data,
                graphic.origin_x,
                graphic.origin_y,
                upm,
                graphic.ppem,
            ));
            break;
        }
        let Some((family, gid, png, ox, oy, upm, ppem)) = found else {
            return Err("no registered face holds an `sbix` picture of it".to_string());
        };
        let key = (family, gid, size_px.to_bits());
        if let Some(hit) = self.emoji.get(&key) {
            return Ok(hit.clone());
        }
        let label = format!("U+{:04X} emoji picture", ch as u32);
        let decoded = ffmpeg::picture::decode_memory(ffmpeg_bin, &png, &label)?;
        let pic = scale_rgba(&decoded, ppem, size_px);
        let made = Arc::new(EmojiPic {
            width: pic.0,
            height: pic.1,
            rgba: Arc::new(pic.2),
            origin_x: ox,
            origin_y: oy,
            units_per_em: upm,
        });
        self.emoji.insert(key, made.clone());
        Ok(made)
    }
}

impl Default for Fonts {
    fn default() -> Self {
        Fonts::new()
    }
}

/// Split parley borrows for one layout: the shaping run needs the font
/// and layout contexts together while faces resolve through methods.
pub(crate) struct FontsRef<'a> {
    pub(crate) context: &'a mut parley::FontContext,
    pub(crate) layouts: &'a mut parley::LayoutContext<[u8; 4]>,
}

impl Fonts {
    pub(crate) fn parts(&mut self) -> FontsRef<'_> {
        FontsRef {
            context: &mut self.context,
            layouts: &mut self.layouts,
        }
    }
}

/// Straight-RGBA picture scaled to the drawn size: each side times the
/// size over the strike's `ppem`, rounded half up, with a std bilinear
/// filter (no linked resizer). Empty for a degenerate target, which
/// draws nothing.
fn scale_rgba(
    decoded: &ffmpeg::picture::RgbaPicture,
    ppem: u16,
    size_px: f32,
) -> (u32, u32, Vec<u8>) {
    if ppem == 0 {
        return (0, 0, Vec::new());
    }
    let factor = f64::from(size_px) / f64::from(ppem);
    let tw = (decoded.width as f64 * factor + 0.5).floor() as u32;
    let th = (decoded.height as f64 * factor + 0.5).floor() as u32;
    if tw == 0 || th == 0 {
        return (0, 0, Vec::new());
    }
    if (tw, th) == (decoded.width, decoded.height) {
        return (tw, th, decoded.bytes.clone());
    }
    let mut out = vec![0u8; tw as usize * th as usize * 4];
    for y in 0..th {
        for x in 0..tw {
            let sx = ((x as f64 + 0.5) * decoded.width as f64 / tw as f64 - 0.5)
                .clamp(0.0, decoded.width as f64 - 1.0);
            let sy = ((y as f64 + 0.5) * decoded.height as f64 / th as f64 - 0.5)
                .clamp(0.0, decoded.height as f64 - 1.0);
            let x0 = sx.floor() as u32;
            let y0 = sy.floor() as u32;
            let x1 = (x0 + 1).min(decoded.width - 1);
            let y1 = (y0 + 1).min(decoded.height - 1);
            let fx = sx - x0 as f64;
            let fy = sy - y0 as f64;
            let at = |xx: u32, yy: u32| -> [f64; 4] {
                let o = (yy * decoded.width + xx) as usize * 4;
                [
                    decoded.bytes[o] as f64,
                    decoded.bytes[o + 1] as f64,
                    decoded.bytes[o + 2] as f64,
                    decoded.bytes[o + 3] as f64,
                ]
            };
            let a = at(x0, y0);
            let b = at(x1, y0);
            let c = at(x0, y1);
            let d = at(x1, y1);
            // Straight alpha: blend color and alpha separately, then
            // clamp each color to its alpha so every pixel stays valid.
            let o = (y * tw + x) as usize * 4;
            let mut px = [0f64; 4];
            for k in 0..4 {
                px[k] = a[k] * (1.0 - fx) * (1.0 - fy)
                    + b[k] * fx * (1.0 - fy)
                    + c[k] * (1.0 - fx) * fy
                    + d[k] * fx * fy;
            }
            let alpha = px[3].round().clamp(0.0, 255.0);
            for k in 0..3 {
                px[k] = px[k].round().clamp(0.0, alpha);
            }
            out[o] = px[0] as u8;
            out[o + 1] = px[1] as u8;
            out[o + 2] = px[2] as u8;
            out[o + 3] = alpha as u8;
        }
    }
    (tw, th, out)
}
