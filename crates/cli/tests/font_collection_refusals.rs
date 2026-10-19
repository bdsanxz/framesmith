//! The refusals of font collections and of colour pictures that `docs/spec/text.md` states (sections 2.6 and 12.4), each
//! checked by `framesmith check` and by `framesmith render`: the code, the reason in the message and the line are those the
//! spec gives, the two commands print the same report, and `render` writes no film. The collections are made in these tests
//! from the bytes of the project's test font (`fixtures/fonts`, ours), so no font of the machine is read and the tests run
//! on every system.

mod common;

use std::fs;
use std::path::Path;

use common::{err, framesmith, out, project};

const GROUP: &str = "font-collection-refusals";
/// The file of the collection in the project folder.
const FONT_FILE: &str = "collection.ttc";
/// The family name of the test font, which is the name of each of its faces.
const FAMILY: &str = "Framesmith Test Stroke";
const REGULAR: &str = "framesmith-test-stroke-400.ttf";
const BOLD: &str = "framesmith-test-stroke-700.ttf";

/// A table of a font: its tag and its bytes.
#[derive(Clone, Debug)]
struct Table {
    tag: [u8; 4],
    data: Vec<u8>,
}

fn be32(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn padded(len: usize) -> usize {
    (len + 3) & !3
}

/// The tables of one of the project's test fonts, from its table directory.
fn test_font(file: &str) -> Vec<Table> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fonts").join(file);
    let font = fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let count = usize::from(u16::from_be_bytes([font[4], font[5]]));
    (0..count)
        .map(|number| {
            let record = 12 + 16 * number;
            let offset = be32(&font, record + 8) as usize;
            let length = be32(&font, record + 12) as usize;
            Table {
                tag: font[record..record + 4].try_into().unwrap(),
                data: font[offset..offset + length].to_vec(),
            }
        })
        .collect()
}

/// The tables of `tables` with the table `tag` given `data`, in place of its own or added.
fn with_table(tables: &[Table], tag: &[u8; 4], data: Vec<u8>) -> Vec<Table> {
    let mut out: Vec<Table> = tables.iter().filter(|table| &table.tag != tag).cloned().collect();
    out.push(Table { tag: *tag, data });
    out
}

fn table_of<'a>(tables: &'a [Table], tag: &[u8; 4]) -> &'a Table {
    tables.iter().find(|table| &table.tag == tag).expect("the font has the table")
}

/// The number of glyphs of the font, from its `maxp` table.
fn glyph_count(tables: &[Table]) -> usize {
    usize::from(u16::from_be_bytes([
        table_of(tables, b"maxp").data[4],
        table_of(tables, b"maxp").data[5],
    ]))
}

/// The bytes of a font of `tables` placed at `base` in its file: its table directory, then its tables, with every
/// offset counted from the start of the file. The table checksums are left at zero, which the reader does not check.
fn font_at(tables: &[Table], base: usize) -> Vec<u8> {
    let mut sorted = tables.to_vec();
    sorted.sort_by_key(|table| table.tag);
    let count = sorted.len();
    let entry = count.ilog2();
    let search = (1_u16 << entry) * 16;
    let mut out = 0x0001_0000_u32.to_be_bytes().to_vec();
    out.extend((count as u16).to_be_bytes());
    out.extend(search.to_be_bytes());
    out.extend((entry as u16).to_be_bytes());
    out.extend((count as u16 * 16 - search).to_be_bytes());
    let mut offset = base + 12 + 16 * count;
    let mut data = Vec::new();
    for table in &sorted {
        out.extend(table.tag);
        out.extend(0_u32.to_be_bytes());
        out.extend((offset as u32).to_be_bytes());
        out.extend((table.data.len() as u32).to_be_bytes());
        data.extend(&table.data);
        data.resize(data.len() + padded(table.data.len()) - table.data.len(), 0);
        offset += padded(table.data.len());
    }
    out.extend(data);
    out
}

/// A font collection of `faces`: the header of the TTC format, then each face's font, with the face's offset in the header.
fn collection(faces: &[Vec<Table>]) -> Vec<u8> {
    let mut base = 12 + 4 * faces.len();
    let mut bases = Vec::new();
    let mut fonts = Vec::new();
    for tables in faces {
        bases.push(base);
        let font = font_at(tables, base);
        base += font.len();
        fonts.push(font);
    }
    let mut out = b"ttcf".to_vec();
    out.extend(0x0001_0000_u32.to_be_bytes());
    out.extend((faces.len() as u32).to_be_bytes());
    for at in bases {
        out.extend((at as u32).to_be_bytes());
    }
    for font in fonts {
        out.extend(font);
    }
    out
}

/// The offset of the table `tag` in the table directory of the face at `face`, changed to `offset` in the file.
fn patch_table_offset(file: &mut [u8], face: usize, tag: &[u8; 4], offset: u32) {
    let count = usize::from(u16::from_be_bytes([file[face + 4], file[face + 5]]));
    let record = (0..count)
        .map(|number| face + 12 + 16 * number)
        .find(|&record| &file[record..record + 4] == tag)
        .expect("the face has the table");
    file[record + 8..record + 12].copy_from_slice(&offset.to_be_bytes());
}

/// A `name` table in format 0 with the given names: platform, encoding, language, name ID and the bytes of the name.
fn name_table(names: &[(u16, u16, u16, u16, Vec<u8>)]) -> Vec<u8> {
    let mut records: Vec<u8> = Vec::new();
    let mut storage: Vec<u8> = Vec::new();
    for (platform, encoding, language, id, text) in names {
        records.extend(platform.to_be_bytes());
        records.extend(encoding.to_be_bytes());
        records.extend(language.to_be_bytes());
        records.extend(id.to_be_bytes());
        records.extend((text.len() as u16).to_be_bytes());
        records.extend((storage.len() as u16).to_be_bytes());
        storage.extend(text);
    }
    let mut out = 0_u16.to_be_bytes().to_vec();
    out.extend((names.len() as u16).to_be_bytes());
    out.extend(((6 + 12 * names.len()) as u16).to_be_bytes());
    out.extend(records);
    out.extend(storage);
    out
}

/// The bytes of `text` in UTF-16, big-endian, as the Windows platform's names are written.
fn utf16(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_be_bytes).collect()
}

/// The name table of a face named `family` on the Windows platform, with the family name and a full name.
fn named(family: &str) -> Vec<u8> {
    name_table(&[
        (3, 1, 0x0409, 1, utf16(family)),
        (3, 1, 0x0409, 4, utf16(&format!("{family} Regular"))),
    ])
}

/// A `sbix` table of one strike at `ppem` pixels per em, in which every glyph has the record `record` returns for it, or
/// no record when it returns none. A record is an origin of (0, 0), the type tag and the data.
fn sbix(count: usize, ppem: u16, record: impl Fn(usize) -> Option<Vec<u8>>) -> Vec<u8> {
    let offsets_len = 4 + 4 * (count + 1);
    let mut data = Vec::new();
    let mut offsets = Vec::with_capacity(count + 1);
    for glyph in 0..count {
        offsets.push((offsets_len + data.len()) as u32);
        if let Some(bytes) = record(glyph) {
            data.extend(bytes);
        }
    }
    offsets.push((offsets_len + data.len()) as u32);
    let mut strike = ppem.to_be_bytes().to_vec();
    strike.extend(72_u16.to_be_bytes());
    for offset in offsets {
        strike.extend(offset.to_be_bytes());
    }
    strike.extend(data);
    let mut out = 1_u16.to_be_bytes().to_vec();
    out.extend(1_u16.to_be_bytes());
    out.extend(1_u32.to_be_bytes());
    out.extend(12_u32.to_be_bytes());
    out.extend(strike);
    out
}

/// A glyph record of `sbix`: the origin (0, 0), the type tag and the data.
fn graphic(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = vec![0, 0, 0, 0];
    out.extend(tag);
    out.extend(data);
    out
}

/// The faces of a collection that draws only colour pictures: the test font, named as its family, with an `sbix` table
/// whose every glyph has the record `record` gives.
fn colour_face(file: &str, record: impl Fn(usize) -> Option<Vec<u8>>) -> Vec<Table> {
    let tables = test_font(file);
    let count = glyph_count(&tables);
    let sbix_table = sbix(count, 20, record);
    with_table(&tables, b"sbix", sbix_table)
}

/// A composition of one `<font>` on line 2, naming `FONT_FILE` with `font_attributes`, and one `<text>` on line 3 in
/// `font-family` that holds `text`.
fn composition(font_attributes: &str, text_family: &str, text: &str) -> String {
    format!(
        "<composition version=\"1\" width=\"320\" height=\"300\" fps=\"10\" duration=\"1\" background=\"#ffffff\">\n\
         <font src=\"{FONT_FILE}\"{font_attributes}/>\n\
         <text left=\"10\" top=\"10\" width=\"300\" font-family=\"{text_family}\" font-size=\"20\" line-height=\"2\" color=\"#000000\">{text}</text>\n\
         </composition>\n"
    )
}

/// The report of one command: its lines that name a code, and whether it wrote a film.
struct Report {
    status: Option<i32>,
    lines: Vec<String>,
    film: bool,
    printed: String,
}

fn film_written(project: &Path) -> bool {
    fs::read_dir(project.join("renders"))
        .map(|entries| {
            entries
                .flatten()
                .any(|entry| entry.path().extension().is_some_and(|ext| ext == "mp4"))
        })
        .unwrap_or(false)
}

/// Runs `check` and then `render` on a project of `composition` and the collection `font`, and reports both.
fn check_and_render(name: &str, composition_text: &str, font: &[u8]) -> (Report, Report) {
    let mut reports = Vec::new();
    for command in ["check", "render"] {
        let (project, cwd) = project(GROUP, &format!("{name}-{command}"), composition_text);
        fs::write(project.join(FONT_FILE), font).unwrap();
        let args: Vec<&str> = if command == "check" {
            vec!["check", project.to_str().unwrap()]
        } else {
            vec!["render", project.to_str().unwrap(), "--backend", "cpu"]
        };
        let output = framesmith(&cwd, args, &[]);
        let printed = format!("{}{}", out(&output), err(&output));
        let lines = printed.lines().filter(|line| line.contains("FS-")).map(str::to_string).collect();
        reports.push(Report {
            status: output.status.code(),
            lines,
            film: film_written(&project),
            printed,
        });
    }
    let render = reports.pop().unwrap();
    let check = reports.pop().unwrap();
    (check, render)
}

/// Asserts that both commands refuse the project with the exit code `status` (1 for a report of the kind invalid, 2 for
/// one of the kind unsupported: `check-command.md`, section 5), print the same report of codes and write no film, and
/// returns that report's first line.
fn refused_the_same_way(check: &Report, render: &Report, status: i32) -> String {
    assert_eq!(check.status, Some(status), "check refuses: {}", check.printed);
    assert_eq!(render.status, Some(status), "render refuses: {}", render.printed);
    assert!(!render.printed.contains("Rendered "), "render prints no film: {}", render.printed);
    assert!(!render.film, "render writes no film: {}", render.printed);
    assert_eq!(check.lines, render.lines, "render gives the report that check gives");
    assert!(!check.lines.is_empty(), "the report names a code: {}", check.printed);
    check.lines[0].clone()
}

#[test]
fn a_collection_whose_header_lies_outside_the_file_is_refused_as_not_a_font() {
    let mut font = b"ttcf".to_vec();
    font.extend(0x0001_0000_u32.to_be_bytes());
    font.extend(1_u32.to_be_bytes());
    font.extend(0x00FF_FFFF_u32.to_be_bytes());
    let (check, render) = check_and_render("header-outside", &composition("", FAMILY, "A"), &font);
    let line = refused_the_same_way(&check, &render, 1);
    assert!(line.starts_with("composition.xml, line 2: FS-0051: invalid:"), "{line}");
    assert!(line.contains("is not a font of a kind this version takes"), "{line}");
}

#[test]
fn a_collection_whose_table_directory_lies_outside_the_file_is_refused_as_not_a_font() {
    let mut font = collection(&[test_font(REGULAR)]);
    let end = font.len() as u32;
    font[12..16].copy_from_slice(&(end - 2).to_be_bytes());
    let (check, render) = check_and_render("directory-outside", &composition("", FAMILY, "A"), &font);
    let line = refused_the_same_way(&check, &render, 1);
    assert!(line.starts_with("composition.xml, line 2: FS-0051: invalid:"), "{line}");
}

#[test]
fn a_collection_whose_name_table_lies_outside_the_file_is_refused_as_not_a_font() {
    let mut font = collection(&[test_font(REGULAR)]);
    let face = be32(&font, 12) as usize;
    patch_table_offset(&mut font, face, b"name", 0x00FF_FFF0);
    let (check, render) = check_and_render("name-outside", &composition("", FAMILY, "A"), &font);
    let line = refused_the_same_way(&check, &render, 1);
    assert!(line.starts_with("composition.xml, line 2: FS-0051: invalid:"), "{line}");
}

#[test]
fn a_collection_face_with_no_family_name_is_refused_as_not_a_font() {
    let mut tables = test_font(REGULAR);
    tables = with_table(
        &tables,
        b"name",
        name_table(&[(3, 1, 0x0409, 4, utf16("Framesmith Test Stroke Regular"))]),
    );
    let font = collection(&[tables]);
    let (check, render) = check_and_render("no-family-name", &composition("", FAMILY, "A"), &font);
    let line = refused_the_same_way(&check, &render, 1);
    assert!(line.starts_with("composition.xml, line 2: FS-0051: invalid:"), "{line}");
}

#[test]
fn a_collection_with_no_face_that_is_not_hidden_is_refused_with_the_number_of_its_faces() {
    let hidden = with_table(&test_font(REGULAR), b"name", named(".Hidden Test"));
    let font = collection(&[hidden]);
    let (check, render) = check_and_render("zero-shown", &composition("", FAMILY, "A"), &font);
    let line = refused_the_same_way(&check, &render, 2);
    assert!(line.starts_with("composition.xml, line 2: FS-0035: unsupported:"), "{line}");
    assert!(
        line.contains("a font collection of 0 faces that are not hidden, where exactly one is drawn"),
        "{line}"
    );
}

#[test]
fn a_collection_with_two_faces_that_are_not_hidden_is_refused_with_the_number_of_its_faces() {
    let font = collection(&[test_font(REGULAR), test_font(BOLD)]);
    let (check, render) = check_and_render("two-shown", &composition("", FAMILY, "A"), &font);
    let line = refused_the_same_way(&check, &render, 2);
    assert!(line.starts_with("composition.xml, line 2: FS-0035: unsupported:"), "{line}");
    assert!(
        line.contains("a font collection of 2 faces that are not hidden, where exactly one is drawn"),
        "{line}"
    );
}

#[test]
fn a_collection_whose_one_face_has_outlines_and_no_colour_pictures_is_refused() {
    let font = collection(&[test_font(REGULAR)]);
    let (check, render) = check_and_render("no-sbix", &composition("", FAMILY, "A"), &font);
    let line = refused_the_same_way(&check, &render, 2);
    assert!(line.starts_with("composition.xml, line 2: FS-0035: unsupported:"), "{line}");
    assert!(
        line.contains("a font collection whose one face has outlines and no colour pictures"),
        "{line}"
    );
}

#[test]
fn a_collection_declared_under_another_family_is_refused_on_the_font_line_with_the_name_of_its_face() {
    let font = collection(&[colour_face(REGULAR, |_| Some(graphic(b"dupe", &[0, 1])))]);
    let (check, render) = check_and_render("other-family", &composition(" family=\"Other Family\"", "Other Family", "A"), &font);
    let line = refused_the_same_way(&check, &render, 2);
    assert!(line.starts_with("composition.xml, line 2: FS-0035: unsupported:"), "{line}");
    assert!(
        line.contains("its one face is named Framesmith Test Stroke, not Other Family"),
        "{line}"
    );
}

#[test]
fn a_glyph_whose_only_graphic_is_a_dupe_is_refused_by_check_and_by_render() {
    let font = collection(&[colour_face(REGULAR, |_| Some(graphic(b"dupe", &[0, 1])))]);
    let (check, render) = check_and_render("dupe-only", &composition(" family=\"Framesmith Test Stroke\"", FAMILY, "A"), &font);
    let line = refused_the_same_way(&check, &render, 2);
    assert!(line.starts_with("composition.xml, line 3: FS-0056: unsupported:"), "{line}");
    assert!(
        line.contains("the character U+0041 has no PNG picture in its font at any size"),
        "{line}"
    );
}

#[test]
fn a_glyph_whose_only_graphic_is_a_flip_is_refused_by_check_and_by_render() {
    let font = collection(&[colour_face(REGULAR, |_| Some(graphic(b"flip", &[0, 1, 2, 3])))]);
    let (check, render) = check_and_render("flip-only", &composition(" family=\"Framesmith Test Stroke\"", FAMILY, "A"), &font);
    let line = refused_the_same_way(&check, &render, 2);
    assert!(line.starts_with("composition.xml, line 3: FS-0056: unsupported:"), "{line}");
    assert!(
        line.contains("the character U+0041 has no PNG picture in its font at any size"),
        "{line}"
    );
}

#[test]
fn a_picture_that_frx7_refuses_is_refused_by_check_and_by_render_with_frx7s_reason() {
    let damaged = b"\x89PNG\r\n\x1a\nthis is not a complete picture".to_vec();
    let font = collection(&[colour_face(REGULAR, |_| Some(graphic(b"png ", &damaged)))]);
    let (check, render) = check_and_render(
        "picture-refused",
        &composition(" family=\"Framesmith Test Stroke\"", FAMILY, "A"),
        &font,
    );
    let line = refused_the_same_way(&check, &render, 2);
    assert!(line.starts_with("composition.xml, line 3: FS-0056: unsupported:"), "{line}");
    assert!(
        line.contains(
            "the picture of the character U+0041 is refused: a picture held in memory is damaged: its chunk  is  runs past the end of the file."
        ),
        "{line}"
    );
}
