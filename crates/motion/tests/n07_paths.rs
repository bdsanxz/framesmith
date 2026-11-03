//! Sprint 7 motion (T008): SVG-subset path parsing, arc-length
//! tables, tangent headings, and same-structure morphs.
//!
//! Hand references: straight chords measure exact, symmetric curves
//! hit their parameter midpoints at half length, and a quarter
//! circle measures `5π`.

use motion::path::{flatten, morph, parse, sample, structure, ErrorKind};

fn table_of(data: &str) -> motion::path::Table {
    flatten(&parse(data).unwrap())
}

#[test]
fn lines_measure_exact_and_point_straight() {
    let table = table_of("M10,20 L110,20");
    assert!((table.total - 100.0).abs() < 1e-9, "got {}", table.total);
    let (p, angle) = sample(&table, 50.0);
    assert!(
        (p[0] - 60.0).abs() < 1e-9 && (p[1] - 20.0).abs() < 1e-9,
        "got {p:?}"
    );
    assert!(
        (angle - 0.0).abs() < 1e-9,
        "eastward heading is 0, got {angle}"
    );
}

#[test]
fn headings_follow_screen_axes() {
    let down = table_of("M0,0 L0,100");
    let (_, angle) = sample(&down, 40.0);
    assert!(
        (angle - 90.0).abs() < 1e-9,
        "screen-down is 90, got {angle}"
    );
    let back = table_of("M100,0 L0,0");
    let (_, angle) = sample(&back, 40.0);
    assert!(angle.abs() - 180.0 < 1e-9, "westward is ±180, got {angle}");
}

#[test]
fn relative_forms_match_absolute_ones() {
    let abs = parse("M10,10 L110,10 l0,90 Z").unwrap();
    let rel = parse("m10,10 l100,0 l0,90 z").unwrap();
    assert_eq!(abs, rel);
    let table = flatten(&abs);
    // 100 right, 90 down, then the diagonal home: 100 + 90 + hypot.
    let want = 100.0 + 90.0 + (100.0f64.hypot(90.0));
    assert!(
        (table.total - want).abs() < 1e-6,
        "got {} want {want}",
        table.total
    );
}

#[test]
fn implicit_pairs_after_m_run_as_lines() {
    let one = parse("M0,0 100,0 100,100").unwrap();
    let two = parse("M0,0 L100,0 L100,100").unwrap();
    assert_eq!(one, two);
    assert_eq!(structure(&one), vec!['M', 'L', 'L']);
}

#[test]
fn square_closes_with_true_perimeter() {
    let table = table_of("M0,0 L100,0 L100,100 L0,100 Z");
    assert!((table.total - 400.0).abs() < 1e-6, "got {}", table.total);
    // Past 300 runs the closing edge: up x=0 from (0,100) home.
    let (p, angle) = sample(&table, 350.0);
    assert!(
        (p[0] - 0.0).abs() < 1e-6 && (p[1] - 50.0).abs() < 1e-6,
        "got {p:?}"
    );
    assert!((angle + 90.0).abs() < 1e-6, "got {angle}");
}

#[test]
fn symmetric_cubic_hits_its_midpoint_at_half_length() {
    // B(0.5) = (50, 75) by hand; symmetry puts it at half length.
    let table = table_of("M0,0 C0,100 100,100 100,0");
    let (p, angle) = sample(&table, table.total / 2.0);
    assert!(
        (p[0] - 50.0).abs() < 0.05 && (p[1] - 75.0).abs() < 0.05,
        "got {p:?}"
    );
    assert!(angle.abs() < 1.0, "midpoint tangent runs east, got {angle}");
}

#[test]
fn symmetric_quad_hits_its_midpoint_at_half_length() {
    // B(0.5) = (50, 50) by hand; symmetry puts it at half length.
    let table = table_of("M0,0 Q50,100 100,0");
    let (p, _) = sample(&table, table.total / 2.0);
    assert!(
        (p[0] - 50.0).abs() < 0.05 && (p[1] - 50.0).abs() < 0.05,
        "got {p:?}"
    );
}

#[test]
fn quarter_arc_measures_five_pi_with_end_tangents() {
    // Quarter circle radius 10 about the origin: length 5π.
    let table = table_of("M10,0 A10,10 0 0 1 0,10");
    let want = 5.0 * std::f64::consts::PI;
    assert!(
        (table.total - want).abs() < 0.05,
        "got {} want {want}",
        table.total
    );
    let (_, start) = sample(&table, 0.0);
    assert!(
        (start - 90.0).abs() < 2.0,
        "arc leaves heading down, got {start}"
    );
    let (_, end) = sample(&table, table.total);
    assert!(
        (end.abs() - 180.0).abs() < 2.0,
        "arc arrives heading west, got {end}"
    );
}

#[test]
fn morph_interpolates_command_numbers() {
    let a = parse("M0,0 L100,0").unwrap();
    let b = parse("M0,100 L100,100").unwrap();
    let mid = morph(&a, &b, 0.5).unwrap();
    assert_eq!(structure(&mid), vec!['M', 'L']);
    let table = flatten(&mid);
    assert!((table.total - 100.0).abs() < 1e-9, "got {}", table.total);
    let (p, _) = sample(&table, 50.0);
    assert!(
        (p[0] - 50.0).abs() < 1e-9 && (p[1] - 50.0).abs() < 1e-9,
        "got {p:?}"
    );
    // Endpoints reproduce the inputs exactly.
    assert_eq!(morph(&a, &b, 0.0).unwrap(), a);
    assert_eq!(morph(&a, &b, 1.0).unwrap(), b);
}

#[test]
fn morph_interpolates_close_targets() {
    // The close target is a command number too: halfway between two
    // stacked squares the loop still measures 400 and shuts at
    // (0, 50), not at either input's home corner.
    let a = parse("M0,0 L100,0 L100,100 L0,100 Z").unwrap();
    let b = parse("M0,100 L100,100 L100,200 L0,200 Z").unwrap();
    let mid = morph(&a, &b, 0.5).unwrap();
    let table = flatten(&mid);
    assert!((table.total - 400.0).abs() < 1e-6, "got {}", table.total);
    let (p, _) = sample(&table, table.total);
    assert!(
        (p[0] - 0.0).abs() < 1e-6 && (p[1] - 50.0).abs() < 1e-6,
        "got {p:?}"
    );
}

#[test]
fn morph_across_structures_names_the_command() {
    let a = parse("M0,0 L100,0").unwrap();
    let b = parse("M0,0 C0,100 100,100 100,0").unwrap();
    let err = morph(&a, &b, 0.5).unwrap_err();
    assert_eq!(err.index, 2);
    assert_eq!((err.from_cmd, err.to_cmd), (Some('L'), Some('C')));
    assert!(err.to_string().contains("`L`") && err.to_string().contains("`C`"));
}

#[test]
fn morph_rejects_flag_changes_naming_the_arc() {
    let a = parse("M10,0 A10,10 0 0 1 0,10").unwrap();
    let b = parse("M10,0 A10,10 0 0 0 0,10").unwrap();
    let err = morph(&a, &b, 0.5).unwrap_err();
    assert!(err.flags);
    assert!(err.to_string().contains("`A`"));
}

#[test]
fn unsupported_commands_fail_naming_the_letter() {
    for (data, letter) in [
        ("M0,0 H50", 'H'),
        ("M0,0 S10,10 20,20", 'S'),
        ("M0,0 T20,20", 'T'),
        ("M0,0 V50", 'V'),
    ] {
        let err = parse(data).unwrap_err();
        assert_eq!(
            err.kind,
            ErrorKind::UnknownCommand(letter),
            "{data}: got {:?}",
            err.kind
        );
        assert!(
            err.to_string().contains(&format!("`{letter}`")),
            "{data}: {err}"
        );
    }
}

#[test]
fn malformed_data_fails_naming_the_command() {
    let err = parse("").unwrap_err();
    assert_eq!(err.kind, ErrorKind::Empty);
    let err = parse("M10").unwrap_err();
    assert_eq!((err.command, err.kind), (Some('M'), ErrorKind::Truncated));
    assert!(err.to_string().contains("`M`"));
    let err = parse("M10,xx").unwrap_err();
    assert_eq!(err.command, Some('M'));
    assert!(err.to_string().contains("`M`"));
    let err = parse("M0,0 C0,0 0,0").unwrap_err();
    assert_eq!(err.command, Some('C'));
    let err = parse("10,10").unwrap_err();
    assert_eq!(err.kind, ErrorKind::StrayNumber);
    let err = parse("M10,10 A10,10 0 2 0 20,20").unwrap_err();
    assert_eq!(err.kind, ErrorKind::BadFlag);
    assert!(err.to_string().contains("`A`"));
}

#[test]
fn degenerate_paths_sample_without_nan() {
    // A bare moveto and coincident lines hold their point heading 0.
    let table = table_of("M10,10");
    assert_eq!(table.total, 0.0);
    let (p, angle) = sample(&table, 0.0);
    assert_eq!((p, angle), ([10.0, 10.0], 0.0));
    let table = table_of("M10,10 L10,10 L20,10");
    let (p, angle) = sample(&table, 0.0);
    assert!(
        (p[0] - 10.0).abs() < 1e-9 && angle.abs() < 1e-9,
        "got {p:?} {angle}"
    );
}
