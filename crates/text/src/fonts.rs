//! The fonts a composition declares (`docs/spec/text.md`, section 2): TrueType files registered under the family name each
//! `<font>` gives, or the name the file itself gives, and the one face of a font collection that is drawn (section 2.6).

use std::sync::Arc;

use parley::fontique::{Blob, Collection, CollectionOptions, FontInfoOverride};

use crate::colour::{read_collection, ColourFace, Refusal};
use crate::damage;
use crate::picture;

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
}

/// The font collection of one composition. It starts with no family: the system's fonts are never read.
pub struct Fonts {
    pub(crate) context: parley::FontContext,
    pub(crate) layouts: parley::LayoutContext<[u8; 4]>,
    pub(crate) faces: Vec<Face>,
    /// The codecs that read the pictures of the emoji: frx7's, from memory.
    pub(crate) codecs: codec::Registry,
    /// The pictures read so far, by glyph and size.
    pub(crate) pictures: picture::Cache,
}

impl Default for Fonts {
    fn default() -> Self {
        Fonts::new()
    }
}

impl Fonts {
    /// A collection with no font in it.
    pub fn new() -> Fonts {
        let mut codecs = codec::Registry::new();
        codecs.register(backend_os::BACKEND);
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
            codecs,
            pictures: picture::Cache::new(),
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
        });
        Ok(())
    }

    /// Whether a face of the family named `name` is registered. The names are matched exactly.
    pub fn has_family(&self, name: &str) -> bool {
        self.faces.iter().any(|face| face.family == name)
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
