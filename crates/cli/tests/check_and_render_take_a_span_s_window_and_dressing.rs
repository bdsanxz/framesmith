//! `framesmith check` and `framesmith render` take a `<span>` with a window, shadows, an outline, figures, upper case or no
//! wrapping: check calls the project valid, render writes the film, and both exit 0 (`docs/spec/text.md`, sections 6.5
//! and 6.6; `docs/spec/render-command.md`, section 4.2, rule 5).

mod common;

use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use common::{err, framesmith, out, project};

const HEAD: &str = r##"<composition version="1" id="film" width="160" height="60" fps="10" duration="0.5" background="#000000">
<font src="fonts/framesmith-test-stroke-400.ttf" family="Framesmith Test Stroke"/>
"##;

#[test]
fn check_and_render_take_each_span_form_and_write_the_film() {
    let forms = [
        ("window", r##"start="0.1" duration="0.2""##),
        ("shadow", r##"text-shadow="2px 2px #ff0000""##),
        ("outline", r##"text-stroke="1px #ff0000""##),
        ("figures", r##"font-variant-numeric="tabular-nums""##),
        ("upper-case", r##"text-transform="uppercase""##),
        ("no-wrap", r##"white-space="nowrap""##),
    ];
    let mut failures = Vec::new();
    for (name, attribute) in forms {
        let text = format!(
            "{HEAD}<text left=\"10\" top=\"10\" font-family=\"Framesmith Test Stroke\" font-size=\"20\" color=\"#ffffff\">AB <span {attribute}>cd 12</span></text>\n</composition>\n"
        );
        let (folder, cwd) = project("check-and-render-take-a-span-s-window-and-dressing", name, &text);
        fs::create_dir_all(folder.join("fonts")).unwrap();
        let file = "framesmith-test-stroke-400.ttf";
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fonts").join(file),
            folder.join("fonts").join(file),
        )
        .unwrap();
        let film = cwd.join("film.mp4");
        let check = framesmith(&cwd, [OsStr::new("check"), folder.as_os_str()], &[("NO_COLOR", "1")]);
        let render = framesmith(
            &cwd,
            [
                OsStr::new("render"),
                folder.as_os_str(),
                OsStr::new("--backend"),
                OsStr::new("cpu"),
                OsStr::new("--output"),
                film.as_os_str(),
            ],
            &[("NO_COLOR", "1")],
        );
        if check.status.code() != Some(0) || !out(&check).starts_with("Ready: ") {
            failures.push(format!("{name}: check: {:?} {} {}", check.status.code(), out(&check), err(&check)));
        }
        if render.status.code() != Some(0) || !film.exists() {
            failures.push(format!(
                "{name}: render: {:?} {} {}",
                render.status.code(),
                out(&render),
                err(&render)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
