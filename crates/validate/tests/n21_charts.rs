//! Sprint 21 T002: native `chart` clips through the public
//! [`validate_folder`] API — the same verdict `check` reports, with
//! the resolved rows asserted on the model.
//!
//! Covers every acceptance criterion of
//! `T002-draw-charts-natively.md`: each kind renders its axes and
//! labels, data files and inline rows both feed charts, reruns are
//! byte-identical on the same data (see the render-side test in
//! `crates/render/src/chart.rs`), malformed rows refuse naming row
//! and field, and empty series render named empty axes.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Diagnostic, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n21charts-{}-{}-{}",
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

const CLOSE: &str = "}\n";

fn diags(body: &str) -> Vec<Diagnostic> {
    let dir = tmp("case");
    write_comp(&dir, body);
    validate_folder(&dir).diagnostics
}

fn codes(ds: &[Diagnostic]) -> Vec<String> {
    ds.iter().map(|d| d.code.to_string()).collect()
}

fn assert_refusal(ds: &[Diagnostic], code: &str, needles: &[&str]) {
    assert!(!ds.is_empty(), "expected a refusal, got none");
    assert!(
        ds.iter().any(|d| d.code == code),
        "expected code {code} in {:?}",
        codes(ds)
    );
    let messages: Vec<String> = ds.iter().map(|d| d.message.clone()).collect();
    for needle in needles {
        assert!(
            messages.iter().any(|m| m.contains(needle)),
            "expected {needle:?} in {messages:?}"
        );
    }
}

fn first_chart(body: &str) -> notation::model::ChartClip {
    let dir = tmp("valid");
    write_comp(&dir, body);
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match v.composition.unwrap().clips.into_iter().next().unwrap() {
        notation::model::Clip::Chart(c) => c,
        other => panic!("expected a chart, got {other:?}"),
    }
}

fn first_chart_in(dir: &Path) -> notation::model::ChartClip {
    let v = validate_folder(dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match v.composition.unwrap().clips.into_iter().next().unwrap() {
        notation::model::Clip::Chart(c) => c,
        other => panic!("expected a chart, got {other:?}"),
    }
}

fn rows_body(kind: &str) -> String {
    format!(
        "{OPEN}    chart kind=\"{kind}\" start=0 duration=2 x=10 y=10 width=300 height=200 {{\n        row label=\"Q1\" value=12\n        row label=\"Q2\" value=19 series=\"a\"\n    }}\n{CLOSE}"
    )
}

/// Each of the eight kinds validates with its axes and labels.
#[test]
fn every_kind_validates_with_axes_and_labels() {
    for kind in [
        "bar", "line", "area", "pie", "donut", "scatter", "radar", "gauge",
    ] {
        let body = if kind == "gauge" {
            format!(
                "{OPEN}    chart kind=\"gauge\" start=0 duration=2 x=10 y=10 width=300 height=200 {{\n        row label=\"load\" value=62\n    }}\n{CLOSE}"
            )
        } else if kind == "pie" || kind == "donut" {
            format!(
                "{OPEN}    chart kind=\"{kind}\" title=\"share\" start=0 duration=2 x=10 y=10 width=300 height=200 {{\n        row label=\"a\" value=30\n        row label=\"b\" value=70\n    }}\n{CLOSE}"
            )
        } else {
            rows_body(kind)
        };
        let c = first_chart(&body);
        assert_eq!(c.kind.name(), kind, "kind round-trips");
        assert!(!c.data.is_empty(), "rows resolve for {kind}");
        if kind == "pie" || kind == "donut" {
            assert_eq!(c.title.as_deref(), Some("share"));
        }
    }
}

/// Inline rows resolve in order with series, labels, values, and
/// scatter x positions.
#[test]
fn inline_rows_resolve_in_order() {
    let body = format!(
        "{OPEN}    chart kind=\"scatter\" start=0 duration=2 {{\n        row label=\"p\" value=1.5 x=0.5 series=\"s\"\n        row label=\"q\" value=2.5 series=\"s\"\n    }}\n{CLOSE}"
    );
    let c = first_chart(&body);
    assert_eq!(c.data.len(), 2);
    assert_eq!(c.data[0].label, "p");
    assert_eq!(c.data[0].value, 1.5);
    assert_eq!(c.data[0].x, Some(0.5));
    assert_eq!(c.data[0].series.as_deref(), Some("s"));
    assert_eq!(c.data[1].x, None);
    // No data file: nothing to digest beyond the inline rows.
    assert_eq!(c.data_file, None);
}

/// Data files feed charts: `label,value` and `series,label,value`
/// rows land in file order.
#[test]
fn data_file_feeds_charts() {
    let dir = tmp("file");
    std::fs::write(dir.join("sales.csv"), "Q1,12\na,Q2,19\n").unwrap();
    write_comp(
        &dir,
        &format!("{OPEN}    chart kind=\"bar\" data=\"sales.csv\" start=0 duration=2\n{CLOSE}"),
    );
    let c = first_chart_in(&dir);
    assert_eq!(c.data_file.as_deref(), Some("sales.csv"));
    assert_eq!(c.data.len(), 2);
    assert_eq!(c.data[0].label, "Q1");
    assert_eq!(c.data[0].value, 12.0);
    assert_eq!(c.data[0].series, None);
    assert_eq!(c.data[1].series.as_deref(), Some("a"));
    assert_eq!(c.data[1].label, "Q2");
}

/// File rows come first, then inline rows.
#[test]
fn file_rows_come_first_then_inline() {
    let dir = tmp("both");
    std::fs::write(dir.join("sales.csv"), "Q1,12\n").unwrap();
    write_comp(
        &dir,
        &format!(
            "{OPEN}    chart kind=\"bar\" data=\"sales.csv\" start=0 duration=2 {{\n        row label=\"Q2\" value=19\n    }}\n{CLOSE}"
        ),
    );
    let c = first_chart_in(&dir);
    assert_eq!(c.data.len(), 2);
    assert_eq!(c.data[0].label, "Q1");
    assert_eq!(c.data[1].label, "Q2");
}

/// A missing data file refuses naming the file before any render work.
#[test]
fn missing_data_file_refuses_naming_the_file() {
    let body =
        format!("{OPEN}    chart kind=\"bar\" data=\"gone.csv\" start=0 duration=2\n{CLOSE}");
    assert_refusal(&diags(&body), "E_MISSING_FILE", &["gone.csv"]);
}

/// A malformed file row refuses naming the file, the row, and the field.
#[test]
fn malformed_file_row_names_file_row_and_field() {
    let dir = tmp("badfile");
    std::fs::write(dir.join("sales.csv"), "Q1,12\nQ2,lots\n,7\n").unwrap();
    write_comp(
        &dir,
        &format!("{OPEN}    chart kind=\"bar\" data=\"sales.csv\" start=0 duration=2\n{CLOSE}"),
    );
    let ds = validate_folder(&dir).diagnostics;
    assert_refusal(&ds, "E_BAD_CHART", &["sales.csv", "row 2", "value"]);
    assert_refusal(&ds, "E_BAD_CHART", &["sales.csv", "row 3", "label"]);
}

/// A malformed inline row refuses naming the row and the field.
#[test]
fn malformed_inline_row_names_row_and_field() {
    let novalue = format!(
        "{OPEN}    chart kind=\"bar\" start=0 duration=2 {{\n        row label=\"Q1\" value=12\n        row label=\"Q2\" value=\"lots\"\n    }}\n{CLOSE}"
    );
    assert_refusal(&diags(&novalue), "E_BAD_CHART", &["row 2", "value"]);
    let nolabel = format!(
        "{OPEN}    chart kind=\"bar\" start=0 duration=2 {{\n        row label=\"Q1\" value=12\n        row value=7\n    }}\n{CLOSE}"
    );
    assert_refusal(&diags(&nolabel), "E_BAD_CHART", &["row 2", "label"]);
}

/// An unknown kind refuses naming the accepted set (fail-closed).
#[test]
fn unknown_kind_refuses_naming_accepted_kinds() {
    let body = format!(
        "{OPEN}    chart kind=\"histogram\" start=0 duration=2 {{\n        row label=\"Q1\" value=1\n    }}\n{CLOSE}"
    );
    assert_refusal(&diags(&body), "E_BAD_CHART", &["histogram", "bar", "gauge"]);
    // A missing kind refuses as well: charts never guess a picture.
    let missing = format!(
        "{OPEN}    chart start=0 duration=2 {{\n        row label=\"Q1\" value=1\n    }}\n{CLOSE}"
    );
    assert_refusal(
        &diags(&missing),
        "E_BAD_CHART",
        &["missing required `kind`"],
    );
}

/// Unknown keys refuse (fail-closed); hyphenated style attributes parse.
#[test]
fn unknown_keys_refuse_and_style_attributes_parse() {
    let body = format!(
        "{OPEN}    chart kind=\"bar\" start=0 duration=2 object-fit=\"cover\" {{\n        row label=\"Q1\" value=1\n    }}\n{CLOSE}"
    );
    assert_refusal(&diags(&body), "E_UNKNOWN_KEY", &["object-fit"]);
    let styled = format!(
        "{OPEN}    chart kind=\"bar\" start=0 duration=2 title=\"sales\" background-color=\"#112233\" border-width=2 border-color=\"#445566\" colors=\"#ff0000,#00ff00\" show-legend=false show-gridlines=false show-value-labels=false {{\n        row label=\"Q1\" value=1\n    }}\n{CLOSE}"
    );
    let c = first_chart(&styled);
    assert_eq!(c.title.as_deref(), Some("sales"));
    assert_eq!(
        c.background,
        notation::model::Rgb {
            r: 0x11,
            g: 0x22,
            b: 0x33
        }
    );
    assert_eq!(c.border_width, 2.0);
    assert_eq!(c.palette.len(), 2);
    assert!(!c.show_legend && !c.show_gridlines && !c.show_value_labels);
}

/// Empty series validate clean and render named empty axes: no rows,
/// the title (or kind name) still carried for the painter.
#[test]
fn empty_series_validates_for_named_empty_axes() {
    let body = format!("{OPEN}    chart kind=\"line\" title=\"empty\" start=0 duration=2\n{CLOSE}");
    let c = first_chart(&body);
    assert!(c.data.is_empty());
    assert_eq!(c.title.as_deref(), Some("empty"));
    // No title: the kind name still names the axes downstream.
    let untitled = format!("{OPEN}    chart kind=\"radar\" start=0 duration=2\n{CLOSE}");
    let c = first_chart(&untitled);
    assert!(c.data.is_empty());
    assert_eq!(c.title, None);
}

/// Multi-series pies and multi-point gauges refuse naming kind and count.
#[test]
fn pie_and_gauge_shape_rules_name_kind_and_count() {
    let pie = format!(
        "{OPEN}    chart kind=\"pie\" start=0 duration=2 {{\n        row label=\"a\" value=1 series=\"x\"\n        row label=\"b\" value=2 series=\"y\"\n    }}\n{CLOSE}"
    );
    assert_refusal(&diags(&pie), "E_BAD_CHART", &["pie", "2", "series"]);
    let gauge = format!(
        "{OPEN}    chart kind=\"gauge\" start=0 duration=2 {{\n        row label=\"a\" value=1\n        row label=\"b\" value=2\n    }}\n{CLOSE}"
    );
    assert_refusal(&diags(&gauge), "E_BAD_CHART", &["gauge", "2"]);
}

/// `min`/`max` bind gauges only; `x` binds scatter rows only.
#[test]
fn min_max_and_x_are_scoped() {
    let minbar = format!(
        "{OPEN}    chart kind=\"bar\" min=0 start=0 duration=2 {{\n        row label=\"a\" value=1\n    }}\n{CLOSE}"
    );
    assert_refusal(&diags(&minbar), "E_BAD_CHART", &["min", "gauge"]);
    let xbar = format!(
        "{OPEN}    chart kind=\"bar\" start=0 duration=2 {{\n        row label=\"a\" value=1 x=2\n    }}\n{CLOSE}"
    );
    assert_refusal(&diags(&xbar), "E_BAD_CHART", &["row 1", "x", "scatter"]);
    // A ranged gauge validates with its bounds on the model.
    let gauge = format!(
        "{OPEN}    chart kind=\"gauge\" min=0 max=100 start=0 duration=2 {{\n        row label=\"load\" value=62\n    }}\n{CLOSE}"
    );
    let c = first_chart(&gauge);
    assert_eq!((c.min, c.max), (Some(0.0), Some(100.0)));
}

/// Same file validates to the same rows on reruns (deterministic input).
#[test]
fn same_data_validates_to_same_rows() {
    let dir = tmp("stable");
    std::fs::write(dir.join("sales.csv"), "Q1,12\na,Q2,19\n").unwrap();
    let body = format!(
        "{OPEN}    chart kind=\"line\" data=\"sales.csv\" start=0 duration=2 {{\n        row label=\"Q3\" value=7 series=\"a\"\n    }}\n{CLOSE}"
    );
    write_comp(&dir, &body);
    let a = first_chart_in(&dir).data;
    let b = first_chart_in(&dir).data;
    assert_eq!(a, b);
    assert_eq!(a.len(), 3);
}
