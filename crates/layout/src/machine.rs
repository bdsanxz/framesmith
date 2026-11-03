//! The fonts of the machine: the font files of the machine's font
//! folders, read by our own reads of their tables. No system interface
//! is called. Family names are read once, the first time a text asks
//! for them; each face's `cmap` is read with its record and kept.
//!
//! Compared with the old engine this scan also covers Linux font
//! folders (the old one read macOS only); the candidate shapes stay the
//! same so the success line keeps naming file basenames.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use glyph::tables::{family_name_of_table, maps_char_in_table};

/// The families the generic names stand for: the macOS names first,
/// then the Linux ones, so one composition falls back on both systems.
const GENERIC: [(&str, &str); 3] = [
    ("sans-serif", "Helvetica"),
    ("serif", "Times"),
    ("monospace", "Menlo"),
];

/// Extra Linux candidates tried when the generic family itself is
/// absent from the machine.
const GENERIC_LINUX: [(&str, &str); 3] = [
    ("sans-serif", "DejaVu Sans"),
    ("serif", "DejaVu Serif"),
    ("monospace", "DejaVu Sans Mono"),
];

/// The families the fallback tries first, in this order, before every
/// other family of the machine.
const FIRST_IN_FALL_BACK: [&str; 3] = ["Helvetica", "Arial Unicode MS", "Apple Color Emoji"];

/// The family a text draws for when no name of its list is present.
pub(crate) const MISSING_MARK_FAMILY: &str = "Helvetica";

/// One face of the machine: its file, its index in the file (0 for a
/// TrueType file), its family name, whether it holds `sbix` pictures,
/// and its raw `cmap` table for coverage reads.
#[derive(Clone, Debug)]
pub(crate) struct MachineFace {
    pub(crate) path: PathBuf,
    pub(crate) index: u32,
    pub(crate) family: String,
    pub(crate) colour: bool,
    pub(crate) cmap: Option<Vec<u8>>,
}

/// The fonts of the machine, in folder reading order.
#[derive(Clone, Debug, Default)]
pub(crate) struct Machine {
    pub(crate) faces: Vec<MachineFace>,
    /// `cmap` bytes read so far, by face place: `None` when unreadable.
    cmaps: HashMap<usize, Option<Vec<u8>>>,
}

/// The folders read, in order. macOS folders first (old-engine parity),
/// then the Linux system folders and the user folder of each system.
fn folders() -> Vec<PathBuf> {
    let mut folders = vec![
        PathBuf::from("/System/Library/Fonts"),
        PathBuf::from("/System/Library/Fonts/Supplemental"),
        PathBuf::from("/Library/Fonts"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        folders.push(Path::new(&home).join("Library").join("Fonts"));
    }
    folders.push(PathBuf::from("/usr/share/fonts/truetype/dejavu"));
    folders.push(PathBuf::from("/usr/share/fonts/truetype"));
    folders.push(PathBuf::from("/usr/share/fonts"));
    if let Some(home) = std::env::var_os("HOME") {
        folders.push(Path::new(&home).join(".fonts"));
    }
    folders
}

/// The `len` bytes of `file` at `at`.
fn read_at(file: &mut File, at: u64, len: usize) -> Option<Vec<u8>> {
    file.seek(SeekFrom::Start(at)).ok()?;
    let mut bytes = vec![0; len];
    file.read_exact(&mut bytes).ok()?;
    Some(bytes)
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// Table directory of the face at `offset`: tag → (table offset, length)
/// pairs validated against the file length.
fn dir_records(file: &mut File, offset: u64, length: u64) -> Option<Vec<([u8; 4], u64, usize)>> {
    let head = read_at(file, offset, 12)?;
    let count = u16_at(&head, 4)? as usize;
    let records = read_at(file, offset + 12, 16 * count)?;
    let mut out = Vec::with_capacity(count);
    for number in 0..count {
        let record = &records[16 * number..16 * number + 16];
        let tag = [record[0], record[1], record[2], record[3]];
        let (Some(at), Some(len)) = (u32_at(record, 8), u32_at(record, 12)) else {
            continue;
        };
        if u64::from(at) + u64::from(len) <= length {
            out.push((tag, u64::from(at), len as usize));
        }
    }
    Some(out)
}

/// The faces taken from one file: its header, each face's table
/// directory, and the `name` table read alone (never the whole file).
fn faces_of_file(path: &Path) -> Option<Vec<MachineFace>> {
    let mut file = File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    let start = read_at(&mut file, 0, 12)?;
    let offsets: Vec<u64> = match &start[..4] {
        b"ttcf" => {
            let count = u64::from(u32_at(&start, 8)?);
            if count == 0 || 12 + 4 * count > length {
                return None;
            }
            let header = read_at(&mut file, 0, (12 + 4 * count) as usize)?;
            let mut out = Vec::new();
            for i in 0..count {
                out.push(u64::from(u32_at(&header, (12 + 4 * i) as usize)?));
            }
            out
        }
        [0, 1, 0, 0] | b"true" | b"OTTO" => vec![0],
        _ => return None,
    };
    let mut faces = Vec::new();
    for (index, offset) in offsets.into_iter().enumerate() {
        let Some(records) = dir_records(&mut file, offset, length) else {
            continue;
        };
        let colour = records.iter().any(|(tag, _, _)| tag == b"sbix");
        if !colour && !records.iter().any(|(tag, _, _)| tag == b"glyf") {
            continue;
        }
        let Some((_, name_off, name_len)) = records.iter().find(|(tag, _, _)| tag == b"name").copied() else {
            continue;
        };
        let Some(name) = read_at(&mut file, name_off, name_len) else {
            continue;
        };
        let Some(family) = family_name_of_table(&name) else {
            continue;
        };
        if family.is_empty() || family.starts_with('.') {
            continue;
        }
        let cmap = records
            .iter()
            .find(|(tag, _, _)| tag == b"cmap")
            .and_then(|(_, off, len)| read_at(&mut file, *off, *len));
        faces.push(MachineFace {
            path: path.to_path_buf(),
            index: index as u32,
            family,
            colour,
            cmap,
        });
    }
    Some(faces)
}

impl Machine {
    /// Scans the folders once. Files sort by raw name bytes; faces keep
    /// file order. Unreadable folders contribute nothing, never an error.
    pub(crate) fn scan() -> Machine {
        let mut faces = Vec::new();
        for folder in folders() {
            let Ok(entries) = std::fs::read_dir(&folder) else {
                continue;
            };
            let mut files: Vec<PathBuf> = entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.is_file()
                        && path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                            e.eq_ignore_ascii_case("ttf") || e.eq_ignore_ascii_case("ttc") || e.eq_ignore_ascii_case("otf")
                        })
                })
                .collect();
            files.sort_by(|a, b| a.as_os_str().as_encoded_bytes().cmp(b.as_os_str().as_encoded_bytes()));
            for path in files {
                faces.extend(faces_of_file(&path).unwrap_or_default());
            }
        }
        Machine { faces, cmaps: HashMap::new() }
    }

    /// The generic candidates for `name`: the macOS family, then the
    /// Linux one, so `sans-serif` resolves on both systems.
    pub(crate) fn generic(name: &str) -> Option<(&'static str, &'static str)> {
        GENERIC.into_iter().find(|(generic, _)| *generic == name)
    }

    pub(crate) fn generic_linux(name: &str) -> Option<&'static str> {
        GENERIC_LINUX.into_iter().find(|(generic, _)| *generic == name).map(|(_, family)| family)
    }

    /// The family of the machine that `name` names: matched without
    /// regard to the case of the letters A to Z.
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

    /// The families of the fallback, in order: the three tried first
    /// where the machine has them, then every other family at the place
    /// of its first face.
    pub(crate) fn fall_back_order(&self) -> Vec<&str> {
        let mut order: Vec<&str> = FIRST_IN_FALL_BACK.iter().filter_map(|name| self.family(name)).collect();
        for face in &self.faces {
            if !order.contains(&face.family.as_str()) {
                order.push(face.family.as_str());
            }
        }
        order
    }

    /// Whether a face of `family` maps `character` past glyph 0.
    pub(crate) fn family_maps(&mut self, family: &str, character: char) -> bool {
        self.faces_of(family).into_iter().any(|at| self.face_maps(at, character))
    }

    /// Whether the face at `at` maps `character` past glyph 0.
    fn face_maps(&mut self, at: usize, character: char) -> bool {
        match self.cmaps.entry(at).or_insert_with(|| self.faces[at].cmap.clone()) {
            Some(cmap) => maps_char_in_table(cmap, character),
            None => false,
        }
    }
}
