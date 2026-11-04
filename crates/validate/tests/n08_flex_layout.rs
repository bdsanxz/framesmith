//! Sprint 8 T001: flex rows and columns through the public
//! [`validate_folder`] API — the same verdict `check` reports, with
//! the resolved model asserted for layout attributes.
//!
//! Covers every acceptance criterion of
//! `T001-flow-rows-and-columns-with-flexbox.md`: row and column
//! offsets (hand math lives in `notation::flex` unit tests and
//! `render` frame tests; here the model carries the exact inputs),
//! centered distribution, both spellings of every layout attribute,
//! and each refusal variant naming its property with a concrete fix.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Diagnostic, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n08flex-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_comp(dir: &Path, body: &str) {
    std::fs::write(dir.join("composition.kdl"), body).unwrap();
}

const OPEN: &str =
    "composition name=\"main\" width=640 height=480 duration=2 background=\"#000\" version=1 {\n";

fn flex_row_three() -> String {
    format!(
        "{OPEN}\
         flex display=\"flex\" flex-direction=\"row\" gap=10 x=10 y=20 width=340 height=100 start=0 duration=2 {{\n\
         box width=100 height=50 fill=\"#ff0000\"\n\
         box width=100 height=50 fill=\"#00ff00\"\n\
         box width=100 height=50 fill=\"#0000ff\"\n\
         }}\n\
         }}\n"
    )
}

fn diags(body: &str) -> Vec<Diagnostic> {
    let dir = tmp("case");
    write_comp(&dir, body);
    validate_folder(&dir).diagnostics
}

fn codes(ds: &[Diagnostic]) -> Vec<String> {
    ds.iter().map(|d| d.code.to_string()).collect()
}

// --- valid layouts --------------------------------------------------------

#[test]
fn row_of_three_boxes_with_gap_is_valid() {
    let dir = tmp("row");
    write_comp(&dir, &flex_row_three());
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert_eq!(comp.clips.len(), 1);
    match &comp.clips[0] {
        notation::model::Clip::Flex(f) => {
            assert_eq!(f.direction, notation::model::FlexDirection::Row);
            assert!((f.gap - 10.0).abs() < 1e-9);
            assert_eq!(f.children.len(), 3);
            assert!((f.children[0].width - 100.0).abs() < 1e-9);
        }
        other => panic!("expected a flex container, got {}", other.kind()),
    }
}

#[test]
fn column_with_flex_grow_is_valid() {
    let dir = tmp("col");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             flex display=\"flex\" flex-direction=\"column\" gap=10 padding=5 x=0 y=0 width=200 height=300 start=0 duration=2 {{\n\
             box width=100 height=50 flex-grow=1 fill=\"#ff0000\"\n\
             box width=100 height=50 flex-grow=2 fill=\"#00ff00\"\n\
             box width=100 height=50 flex-grow=1 fill=\"#0000ff\"\n\
             }}\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match &v.composition.unwrap().clips[0] {
        notation::model::Clip::Flex(f) => {
            assert_eq!(f.direction, notation::model::FlexDirection::Column);
            assert!((f.children[1].flex_grow - 2.0).abs() < 1e-9);
        }
        other => panic!("expected a flex container, got {}", other.kind()),
    }
}

#[test]
fn centered_align_and_justify_are_valid() {
    let dir = tmp("center");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             flex display=\"flex\" flex-direction=\"row\" align-items=\"center\" justify-content=\"center\" x=0 y=0 width=320 height=100 start=0 duration=2 {{\n\
             box width=100 height=40 fill=\"#ff0000\"\n\
             box width=100 height=40 fill=\"#00ff00\"\n\
             }}\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
}

#[test]
fn every_layout_attribute_accepts_underscored_spellings() {
    let dir = tmp("under");
    write_comp(
        &dir,
        &format!(
            "{OPEN}\
             flex display=\"flex\" flex_direction=\"column\" gap=4 padding=2 margin=1 align_items=\"center\" justify_content=\"end\" x=0 y=0 width=200 height=300 start=0 duration=2 {{\n\
             box width=100 height=50 flex_grow=1 fill=\"#ff0000\" border_width=1 border_color=\"#000000\"\n\
             box width=100 height=50 flex_grow=1 fill=\"#00ff00\"\n\
             }}\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match &v.composition.unwrap().clips[0] {
        notation::model::Clip::Flex(f) => {
            assert_eq!(f.direction, notation::model::FlexDirection::Column);
            assert_eq!(f.align_items, notation::model::FlexAlign::Center);
            assert_eq!(
                f.justify_content,
                notation::model::FlexJustify::End
            );
            assert!((f.padding - 2.0).abs() < 1e-9);
            assert!((f.margin - 1.0).abs() < 1e-9);
            assert!((f.children[0].flex_grow - 1.0).abs() < 1e-9);
            assert!((f.children[0].border_width - 1.0).abs() < 1e-9);
        }
        other => panic!("expected a flex container, got {}", other.kind()),
    }
}

#[test]
fn empty_and_single_child_containers_are_valid_both_directions() {
    for direction in ["row", "column"] {
        let dir = tmp("empty");
        write_comp(
            &dir,
            &format!(
                "{OPEN}\
                 flex display=\"flex\" flex-direction=\"{direction}\" x=0 y=0 width=200 height=200 start=0 duration=2 {{\n\
                 }}\n\
                 }}\n"
            ),
        );
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
        match &v.composition.unwrap().clips[0] {
            notation::model::Clip::Flex(f) => assert!(f.children.is_empty()),
            other => panic!("expected a flex container, got {}", other.kind()),
        }
        let dir = tmp("single");
        write_comp(
            &dir,
            &format!(
                "{OPEN}\
                 flex display=\"flex\" flex-direction=\"{direction}\" gap=8 padding=4 align-items=\"center\" justify-content=\"center\" x=0 y=0 width=200 height=200 start=0 duration=2 {{\n\
                 box width=50 height=60 fill=\"#ff0000\"\n\
                 }}\n\
                 }}\n"
            ),
        );
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    }
}

// --- refusals --------------------------------------------------------------

fn assert_refusal(ds: &[Diagnostic], code: &str, property: &str) {
    assert!(!ds.is_empty(), "expected a `{code}` refusal for `{property}`");
    for d in ds {
        assert_eq!(d.code, code, "unexpected code in {d}");
        assert!(
            d.message.contains(property),
            "refusal must name `{property}`: {d}"
        );
        assert!(!d.fix.is_empty(), "refusal must carry a fix: {d}");
        assert!(
            d.fix.contains(property) || d.fix.contains("flex"),
            "fix must name the property or the container: {d}"
        );
    }
}

#[test]
fn missing_flex_direction_fails_naming_the_property() {
    let ds = diags(&format!(
        "{OPEN}\
         flex display=\"flex\" x=0 y=0 width=200 height=100 start=0 duration=2 {{\n\
         box width=50 height=50 fill=\"#ff0000\"\n\
         }}\n\
         }}\n"
    ));
    assert_refusal(&ds, "E_BAD_FLEX", "flex-direction");
}

#[test]
fn bad_flex_direction_fails_naming_the_accepted_set() {
    let ds = diags(&format!(
        "{OPEN}\
         flex display=\"flex\" flex-direction=\"diagonal\" x=0 y=0 width=200 height=100 start=0 duration=2 {{\n\
         box width=50 height=50 fill=\"#ff0000\"\n\
         }}\n\
         }}\n"
    ));
    assert_refusal(&ds, "E_BAD_FLEX", "flex-direction");
    assert!(
        ds[0].message.contains("`row`") && ds[0].message.contains("`column`"),
        "must name the accepted set: {}",
        ds[0]
    );
}

#[test]
fn bad_align_and_justify_fail_naming_the_accepted_set() {
    let ds = diags(&format!(
        "{OPEN}\
         flex display=\"flex\" flex-direction=\"row\" align-items=\"middle\" x=0 y=0 width=200 height=100 start=0 duration=2 {{\n\
         box width=50 height=50 fill=\"#ff0000\"\n\
         }}\n\
         }}\n"
    ));
    assert_refusal(&ds, "E_BAD_FLEX", "align-items");
    let ds = diags(&format!(
        "{OPEN}\
         flex display=\"flex\" flex-direction=\"row\" justify-content=\"space-between\" x=0 y=0 width=200 height=100 start=0 duration=2 {{\n\
         box width=50 height=50 fill=\"#ff0000\"\n\
         }}\n\
         }}\n"
    ));
    assert_refusal(&ds, "E_BAD_FLEX", "justify-content");
}

#[test]
fn negative_spacings_fail_naming_the_number_rule() {
    for key in ["gap", "padding", "margin"] {
        let ds = diags(&format!(
            "{OPEN}\
             flex display=\"flex\" flex-direction=\"row\" {key}=-4 x=0 y=0 width=200 height=100 start=0 duration=2 {{\n\
             box width=50 height=50 fill=\"#ff0000\"\n\
             }}\n\
             }}\n"
        ));
        assert_refusal(&ds, "E_BAD_FLEX", key);
        assert!(
            ds[0].message.contains("0 and up"),
            "must name the number rule: {}",
            ds[0]
        );
    }
}

#[test]
fn non_numeric_spacings_fail_naming_the_number_rule() {
    for key in ["gap", "padding", "margin"] {
        let ds = diags(&format!(
            "{OPEN}\
             flex display=\"flex\" flex-direction=\"row\" {key}=\"wide\" x=0 y=0 width=200 height=100 start=0 duration=2 {{\n\
             box width=50 height=50 fill=\"#ff0000\"\n\
             }}\n\
             }}\n"
        ));
        assert_refusal(&ds, "E_BAD_FLEX", key);
    }
}

#[test]
fn bad_flex_grow_fails_naming_the_number_rule() {
    let ds = diags(&format!(
        "{OPEN}\
         flex display=\"flex\" flex-direction=\"row\" x=0 y=0 width=200 height=100 start=0 duration=2 {{\n\
         box width=50 height=50 flex-grow=-1 fill=\"#ff0000\"\n\
         }}\n\
         }}\n"
    ));
    assert_refusal(&ds, "E_BAD_FLEX", "flex-grow");
    let ds = diags(&format!(
        "{OPEN}\
         flex display=\"flex\" flex-direction=\"row\" x=0 y=0 width=200 height=100 start=0 duration=2 {{\n\
         box width=50 height=50 flex-grow=\"lots\" fill=\"#ff0000\"\n\
         }}\n\
         }}\n"
    ));
    assert_refusal(&ds, "E_BAD_FLEX", "flex-grow");
}

#[test]
fn unknown_container_property_fails_with_style_name_unknown() {
    let ds = diags(&format!(
        "{OPEN}\
         flex display=\"flex\" flex-direction=\"row\" float=\"left\" x=0 y=0 width=200 height=100 start=0 duration=2 {{\n\
         box width=50 height=50 fill=\"#ff0000\"\n\
         }}\n\
         }}\n"
    ));
    assert_eq!(codes(&ds), vec!["STYLE-NAME-UNKNOWN".to_string()]);
    assert!(
        ds[0].message.contains("`float`"),
        "must name the offending property: {}",
        ds[0]
    );
    assert!(
        ds[0].message.contains("closest valid property"),
        "must name the closest valid property: {}",
        ds[0]
    );
    assert!(!ds[0].fix.is_empty());
}

#[test]
fn unknown_child_property_fails_with_style_name_unknown() {
    let ds = diags(&format!(
        "{OPEN}\
         flex display=\"flex\" flex-direction=\"row\" x=0 y=0 width=200 height=100 start=0 duration=2 {{\n\
         box width=50 height=50 clear=\"both\" fill=\"#ff0000\"\n\
         }}\n\
         }}\n"
    ));
    assert_eq!(codes(&ds), vec!["STYLE-NAME-UNKNOWN".to_string()]);
    assert!(ds[0].message.contains("`clear`"), "{}", ds[0]);
}

#[test]
fn child_window_and_position_keys_fail_as_inherited() {
    for key in ["x", "start", "duration"] {
        let prop = if key == "x" {
            "x=5".to_string()
        } else if key == "start" {
            "start=0".to_string()
        } else {
            "duration=1".to_string()
        };
        let ds = diags(&format!(
            "{OPEN}\
             flex display=\"flex\" flex-direction=\"row\" x=0 y=0 width=200 height=100 start=0 duration=2 {{\n\
             box width=50 height=50 {prop} fill=\"#ff0000\"\n\
             }}\n\
             }}\n"
        ));
        assert_refusal(&ds, "E_BAD_FLEX", key);
    }
}

#[test]
fn paint_on_the_container_fails_naming_the_move() {
    let ds = diags(&format!(
        "{OPEN}\
         flex display=\"flex\" flex-direction=\"row\" fill=\"#ff0000\" x=0 y=0 width=200 height=100 start=0 duration=2 {{\n\
         box width=50 height=50 fill=\"#ff0000\"\n\
         }}\n\
         }}\n"
    ));
    assert_refusal(&ds, "E_BAD_FLEX", "fill");
}

#[test]
fn non_box_children_and_nested_flex_fail() {
    let ds = diags(&format!(
        "{OPEN}\
         flex display=\"flex\" flex-direction=\"row\" x=0 y=0 width=200 height=100 start=0 duration=2 {{\n\
         text value=\"Hi\" size=24\n\
         }}\n\
         }}\n"
    ));
    assert!(!ds.is_empty(), "a text child must fail");
    assert!(
        ds.iter().all(|d| d.code == "E_BAD_FLEX"),
        "{ds:?}"
    );
    let ds = diags(&format!(
        "{OPEN}\
         flex display=\"flex\" flex-direction=\"row\" x=0 y=0 width=200 height=100 start=0 duration=2 {{\n\
         flex display=\"flex\" flex-direction=\"column\" width=100 height=50 {{\n\
         }}\n\
         }}\n\
         }}\n"
    ));
    assert_refusal(&ds, "E_BAD_FLEX", "flex");
}

#[test]
fn bad_display_and_sizes_fail() {
    let ds = diags(&format!(
        "{OPEN}\
         flex display=\"block\" flex-direction=\"row\" x=0 y=0 width=200 height=100 start=0 duration=2 {{\n\
         box width=50 height=50 fill=\"#ff0000\"\n\
         }}\n\
         }}\n"
    ));
    assert_refusal(&ds, "E_BAD_FLEX", "display");
    let ds = diags(&format!(
        "{OPEN}\
         flex display=\"flex\" flex-direction=\"row\" x=0 y=0 height=100 start=0 duration=2 {{\n\
         box width=50 height=50 fill=\"#ff0000\"\n\
         }}\n\
         }}\n"
    ));
    assert_refusal(&ds, "E_BAD_FLEX", "width");
    let ds = diags(&format!(
        "{OPEN}\
         flex display=\"flex\" flex-direction=\"row\" x=0 y=0 width=200 height=100 start=0 duration=2 {{\n\
         box height=50 fill=\"#ff0000\"\n\
         }}\n\
         }}\n"
    ));
    assert_refusal(&ds, "E_BAD_FLEX", "width");
}
