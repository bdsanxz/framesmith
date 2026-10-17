//! The fonts a composition declares (`docs/spec/text.md`, section 2): TrueType files registered under the
//! family name each `<font>` gives, or the name the file itself gives.

use std::sync::Arc;

use parley::fontique::{Blob, Collection, CollectionOptions, FontInfoOverride};

/// Why a file is not registered as a font.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontError {
    /// A font of a kind this version does not draw, named by the kind: OpenType with PostScript outlines, WOFF2
    /// or a collection (`docs/spec/text.md`, section 2.1).
    Unsupported(&'static str),
    /// The bytes are not a font of any kind.
    NotAFont,
}

/// One face registered in the collection: the family it belongs to, the file's blob and bytes, and the index.
pub(crate) struct Face {
    pub(crate) family: String,
    pub(crate) blob: u64,
    pub(crate) bytes: Arc<Vec<u8>>,
    pub(crate) index: u32,
}

/// The font collection of one composition. It starts with no family: the system's fonts are never read.
pub struct Fonts {
    pub(crate) context: parley::FontContext,
    pub(crate) layouts: parley::LayoutContext<[u8; 4]>,
    pub(crate) faces: Vec<Face>,
}

impl Default for Fonts {
    fn default() -> Self {
        Fonts::new()
    }
}

impl Fonts {
    /// A collection with no font in it.
    pub fn new() -> Fonts {
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
        }
    }

    /// Registers the bytes of a font file. With `family`, the face is named by it; without, by the family
    /// name the file gives itself. Only a TrueType font is registered: the first four bytes are `0x00010000`
    /// or `true`.
    pub fn register(&mut self, bytes: Vec<u8>, family: Option<&str>) -> Result<(), FontError> {
        match bytes.get(..4) {
            Some([0, 1, 0, 0]) | Some(b"true") => {}
            Some(b"OTTO") => return Err(FontError::Unsupported("OpenType with PostScript outlines")),
            Some(b"wOF2") => return Err(FontError::Unsupported("WOFF2")),
            Some(b"ttcf") => return Err(FontError::Unsupported("a font collection")),
            _ => return Err(FontError::NotAFont),
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
            return Err(FontError::NotAFont);
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
            });
        }
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
