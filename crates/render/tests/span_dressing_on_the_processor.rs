//! The processor draws a span's shadows and outline round the span's own glyphs only (`docs/spec/text.md`, section 6.6,
//! and section 6.5 for a span), worked pixel by pixel from the picture of the same words undressed: a shadow pixel lies
//! at its written offset from a glyph pixel of the span, and an outline pixel within half its width of one. Each text is
//! "HI HI" at 30 pixels from x = 10 with the span on the first "HI". In the test font H and I are 540 units wide and a
//! space 300, so at 0.03 pixel a unit the span's glyphs end by 10 + 1080 x 0.03 = 42.4 and the words after it start at
//! 10 + 1380 x 0.03 = 51.4. A glyph pixel left of x = 47 is the span's; one right of it is the words' after it. The outline
//! checks leave out the rows near the top and the bottom of the glyphs, where the join at a corner decides the picture.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::fixtures;
use draw::Picture;
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const WHITE: [u8; 4] = [255, 255, 255, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];
const BLUE: [u8; 4] = [0x20, 0x50, 0xc0, 255];
const RED: [u8; 4] = [0xc0, 0x20, 0x20, 255];
/// Between the span's glyphs and the words after it.
const SPAN_ENDS: i64 = 47;

/// The picture of frame 0 of a 200 by 60 white canvas holding a black text "HI HI" whose first "HI" is a span written
/// with `dressing`.
fn picture_of(name: &str, dressing: &str) -> Picture {
    let folder: PathBuf = common::scratch("span-dressing-on-the-processor", name);
    fs::create_dir_all(folder.join("fonts")).unwrap();
    let file = "framesmith-test-stroke-400.ttf";
    fs::copy(fixtures().join("fonts").join(file), folder.join("fonts").join(file)).unwrap();
    let composition = format!(
        "<composition version=\"1\" width=\"200\" height=\"60\" fps=\"10\" duration=\"1\" background=\"#ffffff\">\n\
         <font src=\"fonts/{file}\" family=\"Framesmith Test Stroke\"/>\n\
         <text left=\"10\" top=\"8\" font-family=\"Framesmith Test Stroke\" font-size=\"30\" color=\"#000000\"><span {dressing}>HI</span> HI</text>\n\
         </composition>\n"
    );
    fs::write(folder.join("composition.xml"), composition).unwrap();
    scene_picture(&folder)
}

fn scene_picture(folder: &Path) -> Picture {
    let scene = Scene::from_project(&Project::open(folder).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"));
    scene.picture(0, &mut CpuRenderer).unwrap()
}

/// The pixel at (x, y), white outside the picture.
fn at(picture: &Picture, x: i64, y: i64) -> [u8; 4] {
    if x < 0 || y < 0 || x >= i64::from(picture.width()) || y >= i64::from(picture.height()) {
        return WHITE;
    }
    picture.pixel(x as u32, y as u32).unwrap()
}

/// Whether the undressed picture has any ink, black or a shade, within `reach` of (x, y), on the side given.
fn ink_near(plain: &Picture, x: i64, y: i64, reach: i64, in_span: bool) -> bool {
    (-reach..=reach).any(|dy| {
        (-reach..=reach).any(|dx| {
            let (px, py) = (x + dx, y + dy);
            at(plain, px, py) != WHITE && (px < SPAN_ENDS) == in_span
        })
    })
}

/// Whether a pixel beside (x, y), across or up and down, is a whole pixel of a glyph.
fn black_beside(plain: &Picture, x: i64, y: i64) -> bool {
    [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .iter()
        .any(|&(dx, dy)| at(plain, x + dx, y + dy) == BLACK)
}

/// The first and the last row of the glyphs of the undressed picture.
fn glyph_rows(plain: &Picture) -> (i64, i64) {
    let rows: Vec<i64> = (0..i64::from(plain.height()))
        .filter(|&y| (0..i64::from(plain.width())).any(|x| at(plain, x, y) != WHITE))
        .collect();
    (rows[0], *rows.last().unwrap())
}

#[test]
fn a_span_s_shadow_lies_at_its_offset_from_the_span_s_glyphs_only() {
    let plain = picture_of("plain", "");
    let shadowed = picture_of("shadow", "text-shadow=\"6px 4px #2050c0\"");
    let (mut shadow, mut none_under_words) = (0, 0);
    for y in 0..60 {
        for x in 0..200 {
            let drawn = at(&shadowed, x, y);
            let (here, from) = (at(&plain, x, y), at(&plain, x - 6, y - 4));
            if here == BLACK {
                assert_eq!(drawn, BLACK, "the glyph is over the shadow at ({x}, {y})");
            } else if here == WHITE && from == BLACK {
                if x - 6 < SPAN_ENDS {
                    assert_eq!(drawn, BLUE, "the span's shadow at ({x}, {y})");
                    shadow += 1;
                } else {
                    assert_eq!(drawn, WHITE, "no shadow of the words after the span at ({x}, {y})");
                    none_under_words += 1;
                }
            } else if here == WHITE && from == WHITE {
                assert_eq!(drawn, WHITE, "nothing beyond the shadow at ({x}, {y})");
            }
        }
    }
    assert!(
        shadow > 0 && none_under_words > 0,
        "both kinds of place were checked: {shadow}, {none_under_words}"
    );
}

#[test]
fn of_a_span_s_two_shadows_the_first_is_on_top() {
    let plain = picture_of("two-plain", "");
    let shadowed = picture_of("two", "text-shadow=\"6px 4px #2050c0, 2px 2px #c02020\"");
    let (mut overlap, mut only_second) = (0, 0);
    for y in 0..60 {
        for x in 0..SPAN_ENDS + 6 {
            if at(&plain, x, y) != WHITE {
                continue;
            }
            let first = at(&plain, x - 6, y - 4);
            let second = at(&plain, x - 2, y - 2);
            let want = match (first, second) {
                (BLACK, BLACK) => {
                    overlap += 1;
                    BLUE
                }
                (BLACK, WHITE) => BLUE,
                (WHITE, BLACK) => {
                    only_second += 1;
                    RED
                }
                (WHITE, WHITE) => WHITE,
                _ => continue,
            };
            assert_eq!(at(&shadowed, x, y), want, "the shadows at ({x}, {y})");
        }
    }
    assert!(
        overlap > 0 && only_second > 0,
        "both kinds of place were checked: {overlap}, {only_second}"
    );
}

#[test]
fn a_span_s_outline_reaches_half_its_width_round_the_span_s_glyphs_and_round_no_other() {
    let plain = picture_of("outline-plain", "");
    let (top, bottom) = glyph_rows(&plain);
    for (width, reach) in [(2, 1), (4, 2)] {
        let outlined = picture_of(&format!("outline-{width}"), &format!("text-stroke=\"{width}px #2050c0\""));
        let (mut around, mut words) = (0, 0);
        for y in top + 4..=bottom - 4 {
            for x in 0..200 {
                if at(&plain, x, y) != WHITE {
                    continue;
                }
                let drawn = at(&outlined, x, y);
                if x < SPAN_ENDS && black_beside(&plain, x, y) {
                    // A pixel beside a whole pixel of the span's glyph, outside it, lies inside the outline's reach of
                    // the glyph's edge: it is the outline's colour, or a shade of it on white where it is covered in part.
                    assert!(
                        drawn != WHITE && drawn[2] > drawn[0] + 40,
                        "{width} px: the outline at ({x}, {y}): {drawn:?}"
                    );
                    around += 1;
                } else if !ink_near(&plain, x, y, reach, true) {
                    assert_ne!(drawn, BLUE, "{width} px: no outline beyond {reach} pixels at ({x}, {y})");
                    if ink_near(&plain, x, y, reach, false) {
                        assert_eq!(drawn, WHITE, "{width} px: no outline round the words after the span at ({x}, {y})");
                        words += 1;
                    }
                }
            }
        }
        assert!(
            around > 0 && words > 0,
            "{width} px: both kinds of place were checked: {around}, {words}"
        );
    }
}
