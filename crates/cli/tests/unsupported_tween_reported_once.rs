//! A `<tween>` that names both ends of a property this version does not draw, `from-blur` and `to-blur`, is one refusal: `check`
//! reports it once, on the line of the tween, and `render` refuses it with the same single report.

mod common;

use common::{err, framesmith, out, project};

const COMPOSITION: &str = r##"<composition version="1" width="64" height="36" fps="10" duration="1" background="#102030">
<box left="0" top="0" width="8" height="8" background="#ffffff"><tween duration="1" from-blur="20" to-blur="0"/></box>
</composition>
"##;

#[test]
fn a_tween_naming_both_ends_of_an_unsupported_property_is_reported_once_by_check_and_render() {
    let (folder, cwd) = project("unsupported-tween-reported-once", "both-ends", COMPOSITION);
    let checked = framesmith(&cwd, ["check", folder.to_str().unwrap()], &[("NO_COLOR", "1")]);
    let printed = out(&checked);
    assert_eq!(checked.status.code(), Some(2), "{printed}{}", err(&checked));
    // Worked by hand: one report for the tween on line 2, then the closing line.
    let reports: Vec<&str> = printed.lines().filter(|line| line.contains(": unsupported: ")).collect();
    assert_eq!(reports.len(), 1, "check: {printed}");
    assert!(
        reports[0].starts_with("composition.xml, line 2: FS-0035: unsupported: <tween> changes `blur`"),
        "{printed}"
    );
    assert_eq!(printed.lines().count(), 2, "check: {printed}");

    let rendered = framesmith(&cwd, ["render", folder.to_str().unwrap(), "--backend", "cpu"], &[("NO_COLOR", "1")]);
    assert_eq!(out(&rendered), printed, "render prints a different list: {}", err(&rendered));
    assert_eq!(rendered.status.code(), Some(2), "{}", err(&rendered));
}
