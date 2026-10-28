//! The fonts a composition declares (`docs/spec/text.md`, section 2): TrueType files registered under the family name each
//! `<font>` gives, or the name the file itself gives, and the one face of a font collection that is drawn (section 2.6). The
//! faces of the machine's fonts (section 2.7) join it when a text names their family or the fall-back of section 2.4
//! takes them.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use read_fonts::{FontRef, TableProvider};

use parley::fontique::{Blob, Collection, CollectionOptions, FontInfoOverride};

use crate::colour::{read_collection, ColourFace, Refusal};
use crate::damage;
use crate::machine::{self, GENERIC, MISSING_MARK_FAMILY};
use crate::paragraph::TextError;
use crate::picture;

/// The start of the name under which the engine knows a family of the machine, so that no family of a composition is ever
/// taken for one, or joined to one.
const MACHINE_PREFIX: &str = "machine font: ";

/// Why a file is not registered as a font.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontError {
    /// A font of a kind this version does not draw, named by the kind: OpenType with PostScript outlines or WOFF2
    /// (`docs/spec/text.md`, section 2.1).
    Unsupported(&'static str),
    /// A font collection this version does not draw, with the reason (`docs/spec/text.md`, section 2.6).
    Collection(String),
    /// The bytes are not a font of any kind.
    NotAFont,
    /// The file begins as a font of a kind this version reads, and the font reader refuses it: the reader's reason.
    Damaged(String),
}

/// One face registered in the collection: the family it belongs to, the file's blob and bytes, its index in the file, and
/// for a collection the colour pictures of its one face.
pub(crate) struct Face {
    pub(crate) family: String,
    pub(crate) blob: u64,
    pub(crate) bytes: Arc<Vec<u8>>,
    pub(crate) index: u32,
    pub(crate) colour: Option<ColourFace>,
    /// For a face of the machine (`docs/spec/text.md`, section 2.7): its file and its index in that file.
    pub(crate) machine: Option<(PathBuf, u32)>,
}

/// A face of the machine that draws in a collection: its file, its index in that file and its family.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineFace {
    pub path: PathBuf,
    pub index: u32,
    pub family: String,
}

/// The font collection of one composition. It starts with no family: the system's fonts are never read.
pub struct Fonts {
    pub(crate) context: parley::FontContext,
    pub(crate) layouts: parley::LayoutContext<[u8; 4]>,
    pub(crate) faces: Vec<Face>,
    /// The codecs that read the pictures of the emoji, from memory: the registry the caller handed in (`text.md`, section 12.4,
    /// point 4).
    pub(crate) codecs: codec::Registry,
    /// The pictures read so far, by glyph and size.
    pub(crate) pictures: picture::Cache,
    /// The families of the machine registered so far, by their name on the machine: the name the engine knows each by, or
    /// `None` when none of its faces could be registered.
    machine_families: HashMap<String, Option<String>>,
    /// The family of the machine the fall-back gives each character asked so far, by its name on the machine.
    fall_backs: HashMap<char, Option<String>>,
}

impl Fonts {
    /// A collection with no font in it, whose emoji pictures are read by `codecs`, the registry the caller hands in: the
    /// system's backend is registered by the caller, never by this crate (`text.md`, section 12.4, point 4).
    pub fn new(codecs: &codec::Registry) -> Fonts {
        Fonts {
            context: parley::FontContext {
                collection: Collection::new(CollectionOptions {
                    shared: false,
                    system_fonts: false,
                }),
                source_cache: Default::default(),
            },
            layouts: parley::LayoutContext::new(),
            faces: Vec::new(),
            codecs: codecs.clone(),
            pictures: picture::Cache::new(),
            machine_families: HashMap::new(),
            fall_backs: HashMap::new(),
        }
    }

    /// Registers the bytes of a font file. With `family`, the face is named by it; without, by the family
    /// name the file gives itself. A TrueType font is registered: its first four bytes are `0x00010000` or `true`.
    /// A font collection is registered for its one face (`docs/spec/text.md`, section 2.6).
    pub fn register(&mut self, bytes: Vec<u8>, family: Option<&str>) -> Result<(), FontError> {
        match bytes.get(..4) {
            Some([0, 1, 0, 0]) | Some(b"true") => {}
            Some(b"OTTO") => return Err(FontError::Unsupported("OpenType with PostScript outlines")),
            Some(b"wOF2") => return Err(FontError::Unsupported("WOFF2")),
            Some(b"ttcf") => {}
            _ => return Err(FontError::NotAFont),
        }
        if let Some(reason) = damage::refusal(&bytes) {
            return Err(FontError::Damaged(reason));
        }
        if bytes.starts_with(b"ttcf") {
            return self.register_collection(bytes, family);
        }
        let bytes = Arc::new(bytes);
        let blob = Blob::new(bytes.clone());
        let blob_id = blob.id();
        let info_override = family.map(|name| FontInfoOverride {
            family_name: Some(name),
            ..Default::default()
        });
        let registered = self.context.collection.register_fonts(blob, info_override);
        if registered.is_empty() {
            return Err(FontError::Damaged(
                "the font reader reads it, and the shaping engine takes no face from it".into(),
            ));
        }
        for (family_id, _) in registered {
            let name = match family {
                Some(name) => name.to_string(),
                None => self.context.collection.family_name(family_id).unwrap_or_default().to_string(),
            };
            self.faces.push(Face {
                family: name,
                blob: blob_id,
                bytes: bytes.clone(),
                index: 0,
                colour: None,
                machine: None,
            });
        }
        Ok(())
    }

    /// Registers the one face of a font collection that is drawn, read by [`read_collection`]. The engine is given the
    /// whole file with no family override, so that each face keeps its own name, and the hidden faces are never matched by a
    /// family of the composition. The face is then registered at its index in the file.
    fn register_collection(&mut self, bytes: Vec<u8>, family: Option<&str>) -> Result<(), FontError> {
        let colour = match read_collection(&bytes) {
            Ok(colour) => colour,
            Err(Refusal::Invalid) => {
                return Err(FontError::Damaged(
                    "a table of one of its faces lies outside the file or cannot be read, or a face has no family name".into(),
                ))
            }
            Err(Refusal::Unsupported(reason)) => return Err(FontError::Collection(reason)),
        };
        if let Some(name) = family {
            if name != colour.name {
                return Err(FontError::Collection(format!("its one face is named {}, not {name}", colour.name)));
            }
        }
        let index = colour.index;
        let name = colour.name.clone();
        let bytes = Arc::new(bytes);
        let blob = Blob::new(bytes.clone());
        let blob_id = blob.id();
        let registered = self.context.collection.register_fonts(blob, None);
        let at_index = registered.iter().any(|(_, infos)| infos.iter().any(|info| info.index() == index));
        if !at_index {
            return Err(FontError::Damaged(format!(
                "the font reader reads it, and the shaping engine takes no face at index {index} from it"
            )));
        }
        if self.context.collection.family_id(&name).is_none() {
            return Err(FontError::Collection(format!(
                "the engine does not name its one face {name}, so it cannot be matched"
            )));
        }
        self.faces.push(Face {
            family: name,
            blob: blob_id,
            bytes,
            index,
            colour: Some(colour),
            machine: None,
        });
        Ok(())
    }

    /// Whether a face of the family named `name` is registered by the composition. The names are matched exactly.
    pub fn has_family(&self, name: &str) -> bool {
        self.faces.iter().any(|face| face.machine.is_none() && face.family == name)
    }

    /// The families of a `font-family` list as the engine knows them (`docs/spec/text.md`, section 2.3): each name a family
    /// of the composition, a generic family or a family of the machine, in the order of the list, with the absent names
    /// left out. A list whose names are all absent is drawn with `Helvetica` on a machine whose fonts are read, and has no
    /// family on any other.
    pub(crate) fn families_of(&mut self, names: &[String]) -> Result<Vec<String>, TextError> {
        let mut families: Vec<String> = Vec::new();
        for name in names {
            let family = if self.has_family(name) {
                Some(name.clone())
            } else {
                let wanted = GENERIC
                    .iter()
                    .find(|(generic, _)| generic == name)
                    .map_or(name.as_str(), |(_, family)| family);
                machine::machine()
                    .family(wanted)
                    .map(str::to_string)
                    .and_then(|family| self.machine_family(&family))
            };
            if let Some(family) = family {
                if !families.contains(&family) {
                    families.push(family);
                }
            }
        }
        if families.is_empty() {
            match machine::machine().family(MISSING_MARK_FAMILY).map(str::to_string) {
                Some(family) => families.extend(self.machine_family(&family)),
                None => return Err(TextError::NoFamily),
            }
        }
        if families.is_empty() {
            return Err(TextError::NoFamily);
        }
        Ok(families)
    }

    /// Whether a face of one of `families` (names the engine knows) maps `character` to a glyph other than glyph 0
    /// (`docs/spec/text.md`, section 2.4).
    pub(crate) fn families_map(&self, families: &[String], character: char) -> bool {
        self.faces.iter().filter(|face| families.contains(&face.family)).any(|face| {
            FontRef::from_index(&face.bytes, face.index)
                .ok()
                .and_then(|font| font.cmap().ok())
                .and_then(|cmap| cmap.map_codepoint(character))
                .is_some_and(|glyph| glyph.to_u32() != 0)
        })
    }

    /// The family of the machine that the fall-back of `docs/spec/text.md`, section 2.4, gives `character`, registered, as
    /// the engine knows it: the first family in the fall-back's order that maps it. `None` when no family of the machine
    /// maps it, or when the machine's fonts are not read.
    pub(crate) fn fall_back(&mut self, character: char) -> Option<String> {
        let family = match self.fall_backs.get(&character) {
            Some(found) => found.clone(),
            None => {
                let machine = machine::machine();
                let found = machine
                    .fall_back_order()
                    .into_iter()
                    .find(|family| machine.family_maps(family, character))
                    .map(str::to_string);
                self.fall_backs.insert(character, found.clone());
                found
            }
        }?;
        self.machine_family(&family)
    }

    /// Registers the faces of the machine's family `family` (its name on the machine), once, and gives the name the engine
    /// knows it by. A face of colour pictures is registered as a collection is (section 2.6); a face of outlines is
    /// registered alone, from a copy of its file whose header names only that face, under the family's own name with a
    /// prefix, so that it is never taken for a family of the composition.
    fn machine_family(&mut self, family: &str) -> Option<String> {
        if let Some(known) = self.machine_families.get(family) {
            return known.clone();
        }
        let machine = machine::machine();
        let engine_name = format!("{MACHINE_PREFIX}{family}");
        let mut registered = None;
        for at in machine.faces_of(family) {
            let face = &machine.faces[at];
            let Ok(bytes) = std::fs::read(&face.path) else {
                continue;
            };
            if face.colour {
                let before = self.faces.len();
                if self.register_collection(bytes, None).is_ok() {
                    for added in &mut self.faces[before..] {
                        added.machine = Some((face.path.clone(), face.index));
                    }
                    registered = Some(self.faces[before].family.clone());
                }
                continue;
            }
            let Some(alone) = one_face(bytes, face.index) else {
                continue;
            };
            let bytes = Arc::new(alone);
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
            self.faces.push(Face {
                family: engine_name.clone(),
                blob: blob_id,
                bytes,
                index: 0,
                colour: None,
                machine: Some((face.path.clone(), face.index)),
            });
            registered = Some(engine_name.clone());
        }
        tracing::debug!(
            faces = machine.faces_of(family).len(),
            registered = registered.is_some(),
            "machine family registered"
        );
        self.machine_families.insert(family.to_string(), registered.clone());
        registered
    }

    /// The faces of the machine registered so far, in the order they were registered: the faces a text named or the
    /// fall-back took (`docs/spec/text.md`, section 2.7).
    pub fn machine_faces(&self) -> Vec<MachineFace> {
        let machine = machine::machine();
        self.faces
            .iter()
            .filter_map(|face| face.machine.clone())
            .map(|(path, index)| {
                let family = machine
                    .faces
                    .iter()
                    .find(|face| face.path == path && face.index == index)
                    .map(|face| face.family.clone())
                    .unwrap_or_default();
                MachineFace { path, index, family }
            })
            .collect()
    }

    /// The face of the machine whose bytes are `bytes`, if any: how a test names the machine face that drew a run.
    pub fn machine_face_of(&self, bytes: &Arc<Vec<u8>>) -> Option<MachineFace> {
        let face = self.faces.iter().find(|face| Arc::ptr_eq(&face.bytes, bytes))?;
        let (path, index) = face.machine.clone()?;
        let family = machine::machine()
            .faces
            .iter()
            .find(|found| found.path == path && found.index == index)
            .map(|found| found.family.clone())
            .unwrap_or_default();
        Some(MachineFace { path, index, family })
    }

    /// The number of faces registered, of every family.
    pub fn len(&self) -> usize {
        self.faces.len()
    }

    /// Whether no face is registered.
    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    /// The names of the families that the engine's own collection lists, in the engine's order. A test asks it what the
    /// machine's fonts are: none, while the system's fonts are not read (`docs/spec/text.md`, section 10).
    pub fn engine_family_names(&mut self) -> Vec<String> {
        self.context.collection.family_names().map(str::to_string).collect()
    }

    /// Whether the engine's own collection resolves a family of this name.
    pub fn engine_resolves(&mut self, name: &str) -> bool {
        self.context.collection.family_id(name).is_some()
    }
}

/// The bytes of a TrueType file of the face at `index` alone (`docs/spec/text.md`, section 2.7, point 5): a TrueType
/// file as it is, and for a collection a new file of that face's table directory and tables, each table at an offset
/// that is a multiple of four. Both renderers read a TrueType file, and the card reads no collection.
pub fn one_face(bytes: Vec<u8>, index: u32) -> Option<Vec<u8>> {
    if !bytes.starts_with(b"ttcf") {
        return (index == 0).then_some(bytes);
    }
    let be32 = |at: usize| -> Option<usize> { Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize) };
    let face = be32(12 + 4 * usize::try_from(index).ok()?)?;
    let count = usize::from(u16::from_be_bytes(bytes.get(face + 4..face + 6)?.try_into().ok()?));
    let mut alone = bytes.get(face..face + 12)?.to_vec();
    let mut tables = Vec::new();
    let mut at = 12 + 16 * count;
    for number in 0..count {
        let record = face + 12 + 16 * number;
        let (offset, length) = (be32(record + 8)?, be32(record + 12)?);
        let data = bytes.get(offset..offset.checked_add(length)?)?;
        alone.extend_from_slice(bytes.get(record..record + 8)?);
        alone.extend_from_slice(&u32::try_from(at).ok()?.to_be_bytes());
        alone.extend_from_slice(&u32::try_from(length).ok()?.to_be_bytes());
        tables.extend_from_slice(data);
        tables.resize(tables.len().next_multiple_of(4), 0);
        at = 12 + 16 * count + tables.len();
    }
    alone.extend_from_slice(&tables);
    Some(alone)
}
