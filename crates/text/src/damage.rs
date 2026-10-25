//! The font reader's judgement of a declared font file (`docs/spec/text.md`, section 2.1): a file of a kind this version
//! reads is damaged when the reader refuses its header, a table of its faces, or the outline of a glyph.

use read_fonts::tables::glyf::Glyf;
use read_fonts::tables::loca::Loca;
use read_fonts::types::GlyphId;
use read_fonts::{FileRef, FontRef, ReadError, TableProvider};

/// The reason the font reader refuses `bytes`, or `None` when it reads every face of the file.
pub(crate) fn refusal(bytes: &[u8]) -> Option<String> {
    let file = match FileRef::new(bytes) {
        Ok(file) => file,
        Err(error) => return Some(error.to_string()),
    };
    if let FileRef::Collection(collection) = &file {
        // The number of faces the header lists, at bytes 8 to 11 (OpenType, "TTC header"), against the places read.
        let listed = bytes
            .get(8..12)
            .map(|count| u32::from_be_bytes([count[0], count[1], count[2], count[3]]));
        if listed != Some(collection.len()) {
            let listed = listed.unwrap_or(0);
            return Some(format!(
                "its header lists {listed} faces, and the file of {} bytes ends before their places",
                bytes.len()
            ));
        }
    }
    for (number, face) in file.fonts().enumerate() {
        let reason = match face {
            Ok(face) => face_refusal(&face, bytes.len()),
            Err(error) => Some(error.to_string()),
        };
        if let Some(reason) = reason {
            return Some(match file {
                FileRef::Collection(_) => format!("face {number}: {reason}"),
                FileRef::Font(_) => reason,
            });
        }
    }
    None
}

/// The reason the reader refuses one face: a table directory or a table that lies beyond the end of the file, a table the face needs that it
/// cannot read, or a glyph whose outline it cannot read.
fn face_refusal(face: &FontRef, size: usize) -> Option<String> {
    let directory = face.table_directory();
    let listed = usize::from(directory.num_tables());
    if directory.table_records().len() != listed {
        return Some(format!(
            "its table directory of {listed} tables runs past the end of the file of {size} bytes"
        ));
    }
    for record in directory.table_records() {
        let tag = record.tag();
        if face.table_data(tag).is_none() {
            return Some(format!(
                "the table {tag} of {} bytes at byte {} lies beyond the end of the file of {size} bytes",
                record.length(),
                record.offset()
            ));
        }
    }
    let table = |tag: &str, read: Result<(), ReadError>| read.err().map(|error| format!("the table {tag}: {error}"));
    let maxp = match face.maxp() {
        Ok(maxp) => maxp,
        Err(error) => return Some(format!("the table maxp: {error}")),
    };
    let checks = [
        table("head", face.head().map(drop)),
        table("hhea", face.hhea().map(drop)),
        table("hmtx", face.hmtx().map(drop)),
        table("cmap", face.cmap().map(drop)),
        table("name", face.name().map(drop)),
    ];
    if let Some(reason) = checks.into_iter().flatten().next() {
        return Some(reason);
    }
    // A face drawn from outlines: every glyph's outline is read from `loca` and `glyf`. A face of pictures has neither.
    let outlines = |tag: &[u8; 4]| face.table_data(read_fonts::types::Tag::new(tag)).is_some();
    if outlines(b"glyf") || outlines(b"loca") {
        let glyf: Glyf = match face.glyf() {
            Ok(glyf) => glyf,
            Err(error) => return Some(format!("the table glyf: {error}")),
        };
        let loca: Loca = match face.loca(None) {
            Ok(loca) => loca,
            Err(error) => return Some(format!("the table loca: {error}")),
        };
        for number in 0..maxp.num_glyphs() {
            if let Err(error) = loca.get_glyf(GlyphId::new(u32::from(number)), &glyf) {
                return Some(format!("glyph {number}: {error}"));
            }
        }
    }
    None
}
