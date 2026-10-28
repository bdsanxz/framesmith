//! The reports of shadows that are not drawn or not valid (`docs/spec/render-command.md`, section 4.7, rule 8;
//! `docs/spec/check-command.md`): an inner shadow, a spread, and a colour with an alpha below `ff` are unsupported
//! (`FS-0035`), a blur below 0 is invalid (`FS-0034`), and any other form is invalid (`FS-0070`). `check` reports each on its
//! line, `render` refuses them the same way and writes no film.

mod common;

use std::fs;
use std::path::Path;

use common::{err, framesmith, out, project};

const GROUP: &str = "shadow-reports";

/// A composition with one box on each of lines 2 to 6, each with a shadow that is not drawn or not valid.
const COMPOSITION: &str = r##"<composition version="1" width="320" height="120" fps="30" duration="1" background="#ffffff">
<box left="0" top="0" width="40" height="40" background="#ff0000" box-shadow="0 0 4 #00000080 inset"/>
<box left="50" top="0" width="40" height="40" background="#ff0000" box-shadow="0 0 4 2 #000000"/>
<box left="100" top="0" width="40" height="40" background="#ff0000" box-shadow="0 0 4 #00000080"/>
<box left="150" top="0" width="40" height="40" background="#ff0000" box-shadow="0 0 -1 #000000"/>
<box left="200" top="0" width="40" height="40" background="#ff0000" box-shadow="0 0 4 red"/>
</composition>
"##;

fn has_a_film(project: &Path) -> bool {
    fs::read_dir(project.join("renders"))
        .map(|entries| {
            entries
                .flatten()
                .any(|entry| entry.path().extension().is_some_and(|ext| ext == "mp4"))
        })
        .unwrap_or(false)
}

fn coded(printed: &str) -> Vec<String> {
    printed.lines().filter(|line| line.contains("FS-")).map(str::to_string).collect()
}

#[test]
fn shadows_not_drawn_or_not_valid_are_each_reported_on_their_line_by_check_and_render() {
    let (folder, cwd) = project(GROUP, "five", COMPOSITION);
    let checked = framesmith(&cwd, ["check", folder.to_str().unwrap()], &[]);
    let rendered = framesmith(&cwd, ["render", folder.to_str().unwrap(), "--backend", "cpu"], &[]);
    assert_ne!(checked.status.code(), Some(0), "{}{}", out(&checked), err(&checked));
    assert_eq!(
        checked.status.code(),
        rendered.status.code(),
        "{}{}",
        out(&rendered),
        err(&rendered)
    );
    assert!(!has_a_film(&folder), "no film is written");
    let lines = coded(&out(&checked));
    assert_eq!(lines, coded(&out(&rendered)), "render reports what check reports");
    let printed = out(&checked);
    let wanted = [
        ("line 2: FS-0035", "inner shadow"),
        ("line 3: FS-0035", "spread"),
        ("line 4: FS-0035", "alpha below ff"),
        ("line 5: FS-0034", "blur below 0"),
        ("line 6: FS-0070", "not in the form of a shadow"),
    ];
    for (place, words) in wanted {
        let line = printed
            .lines()
            .find(|line| line.contains(place))
            .unwrap_or_else(|| panic!("no report on {place}:\n{printed}"));
        assert!(line.contains(words), "{place} should say {words:?}: {line}");
    }
}
