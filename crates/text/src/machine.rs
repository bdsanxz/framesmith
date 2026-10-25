//! The fonts of the machine (`docs/spec/text.md`, section 2.7): the font files of the machine's font folders, read by our
//! own reading of their tables, on macOS only. No interface of the system is called. The names of the faces are read once,
//! the first time a text asks for them, and kept for the rest of the command; a face's `cmap` table is read when the
//! fall-back of section 2.4 asks it, and kept.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use read_fonts::tables::cmap::Cmap;
use read_fonts::{FontData, FontRead};

use crate::tables::{self, Table};

/// The family the generic names stand for (`docs/spec/text.md`, section 2.7, point 6).
pub(crate) const GENERIC: [(&str, &str); 3] = [("sans-serif", "Helvetica"), ("serif", "Times"), ("monospace", "Menlo")];

/// The families the fall-back tries first, in this order, before every other family of the machine
/// (`docs/spec/text.md`, section 2.4, point 1).
const FIRST_IN_FALL_BACK: [&str; 3] = ["Helvetica", "Arial Unicode MS", "Apple Color Emoji"];

/// The family a face of the machine draws for when no name of a list is present (`docs/spec/text.md`, section 2.4,
/// point 3).
pub(crate) const MISSING_MARK_FAMILY: &str = "Helvetica";

/// One face of the machine: its file, its index in the file (0 for a TrueType file), its family name, whether it is a
/// face of colour pictures, and where its `cmap` table lies in the file.
#[derive(Clone, Debug)]
pub(crate) struct MachineFace {
    pub(crate) path: PathBuf,
    pub(crate) index: u32,
    pub(crate) family: String,
    pub(crate) colour: bool,
    cmap: Option<Table>,
}

/// The fonts of the machine, in the order section 2.7 reads them.
pub(crate) struct Machine {
    pub(crate) faces: Vec<MachineFace>,
    /// The `cmap` table of each face read so far, by its place in `faces`: `None` when it could not be read.
    cmaps: Mutex<HashMap<usize, Option<Arc<Vec<u8>>>>>,
}

/// The fonts of the machine, read the first time they are asked for.
pub(crate) fn machine() -> &'static Machine {
    static MACHINE: OnceLock<Machine> = OnceLock::new();
    MACHINE.get_or_init(|| {
        let faces = folders().iter().flat_map(|folder| faces_of_folder(folder)).collect::<Vec<_>>();
        tracing::debug!(faces = faces.len(), "machine fonts read");
        Machine {
            faces,
            cmaps: Mutex::new(HashMap::new()),
        }
    })
}

/// The folders read, in order (`docs/spec/text.md`, section 2.7, point 1). None on a system other than macOS.
fn folders() -> Vec<PathBuf> {
    if !cfg!(target_os = "macos") {
        return Vec::new();
    }
    let mut folders = vec![
        PathBuf::from("/System/Library/Fonts"),
        PathBuf::from("/System/Library/Fonts/Supplemental"),
        PathBuf::from("/Library/Fonts"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        folders.push(Path::new(&home).join("Library").join("Fonts"));
    }
    folders
}

/// The faces taken from the files of one folder, in the order of the bytes of the files' names (points 2 and 3).
fn faces_of_folder(folder: &Path) -> Vec<MachineFace> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("ttf") || extension.eq_ignore_ascii_case("ttc"))
        })
        .collect();
    files.sort_by(|a, b| a.as_os_str().as_encoded_bytes().cmp(b.as_os_str().as_encoded_bytes()));
    files.iter().flat_map(|path| faces_of_file(path).unwrap_or_default()).collect()
}

/// The `len` bytes of `file` at `at`.
fn read_at(file: &mut File, at: u64, len: usize) -> Option<Vec<u8>> {
    file.seek(SeekFrom::Start(at)).ok()?;
    let mut bytes = vec![0; len];
    file.read_exact(&mut bytes).ok()?;
    Some(bytes)
}

/// The faces of one file that are taken (point 3), read without reading the whole file: its header, the table directory
/// of each face and its `name` table.
fn faces_of_file(path: &Path) -> Option<Vec<MachineFace>> {
    let mut file = File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    let start = read_at(&mut file, 0, 12)?;
    let offsets: Vec<u64> = match &start[..4] {
        b"ttcf" => {
            let count = u64::from(tables::u32_at(&start, 8)?);
            if count == 0 || 12 + 4 * count > length {
                return None;
            }
            let header = read_at(&mut file, 0, usize::try_from(12 + 4 * count).ok()?)?;
            tables::face_offsets(&header)?.into_iter().map(|offset| offset as u64).collect()
        }
        [0, 1, 0, 0] | b"true" => vec![0],
        _ => return None,
    };
    let mut faces = Vec::new();
    for (index, offset) in offsets.into_iter().enumerate() {
        let Some(directory) = read_at(&mut file, offset, 12) else {
            continue;
        };
        let count = usize::from(tables::u16_at(&directory, 4)?);
        let Some(records) = read_at(&mut file, offset + 12, 16 * count) else {
            continue;
        };
        let mut found: HashMap<[u8; 4], Table> = HashMap::new();
        for number in 0..count {
            let record = &records[16 * number..16 * number + 16];
            let tag = [record[0], record[1], record[2], record[3]];
            let (Some(at), Some(len)) = (tables::u32_at(record, 8), tables::u32_at(record, 12)) else {
                continue;
            };
            if u64::from(at) + u64::from(len) <= length {
                found.insert(
                    tag,
                    Table {
                        offset: at as usize,
                        length: len as usize,
                    },
                );
            }
        }
        let colour = found.contains_key(b"sbix");
        if !colour && !found.contains_key(b"glyf") {
            continue;
        }
        let Some(name) = found.get(b"name") else {
            continue;
        };
        let Some(bytes) = read_at(&mut file, name.offset as u64, name.length) else {
            continue;
        };
        let Some(family) = tables::family_name(
            &bytes,
            Table {
                offset: 0,
                length: bytes.len(),
            },
        ) else {
            continue;
        };
        if family.is_empty() || family.starts_with('.') {
            continue;
        }
        faces.push(MachineFace {
            path: path.to_path_buf(),
            index: index as u32,
            family,
            colour,
            cmap: found.get(b"cmap").copied(),
        });
    }
    Some(faces)
}

impl Machine {
    /// The family of the machine that `name` names, as the faces name it: matched without regard to the case of the
    /// letters A to Z (`docs/spec/text.md`, section 2.3, point 3).
    pub(crate) fn family(&self, name: &str) -> Option<&str> {
        self.faces
            .iter()
            .find(|face| face.family.eq_ignore_ascii_case(name))
            .map(|face| face.family.as_str())
    }

    /// The places, in `faces`, of the faces of `family`, in reading order.
    pub(crate) fn faces_of(&self, family: &str) -> Vec<usize> {
        (0..self.faces.len()).filter(|&at| self.faces[at].family == family).collect()
    }

    /// The families of the fall-back, in its order (`docs/spec/text.md`, section 2.4, point 1): the three it tries first,
    /// where the machine has them, then every other family at the place of its first face.
    pub(crate) fn fall_back_order(&self) -> Vec<&str> {
        let mut order: Vec<&str> = FIRST_IN_FALL_BACK.iter().filter_map(|name| self.family(name)).collect();
        for face in &self.faces {
            if !order.contains(&face.family.as_str()) {
                order.push(face.family.as_str());
            }
        }
        order
    }

    /// Whether a face of `family` maps `character` to a glyph other than glyph 0 (`docs/spec/text.md`, section 2.4).
    pub(crate) fn family_maps(&self, family: &str, character: char) -> bool {
        self.faces_of(family).into_iter().any(|at| self.face_maps(at, character))
    }

    /// Whether the face at `at` maps `character` to a glyph other than glyph 0.
    fn face_maps(&self, at: usize, character: char) -> bool {
        let Some(bytes) = self.cmap(at) else {
            return false;
        };
        match Cmap::read(FontData::new(&bytes)) {
            Ok(cmap) => cmap.map_codepoint(character).is_some_and(|glyph| glyph.to_u32() != 0),
            Err(_) => false,
        }
    }

    /// The `cmap` table of the face at `at`, read the first time it is asked for.
    fn cmap(&self, at: usize) -> Option<Arc<Vec<u8>>> {
        let mut cmaps = self.cmaps.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        cmaps
            .entry(at)
            .or_insert_with(|| {
                let face = &self.faces[at];
                let table = face.cmap?;
                let mut file = File::open(&face.path).ok()?;
                read_at(&mut file, table.offset as u64, table.length).map(Arc::new)
            })
            .clone()
    }
}
