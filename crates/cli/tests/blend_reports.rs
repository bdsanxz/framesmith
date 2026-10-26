//! An unknown blend name (`docs/spec/blends.md`, section 1): `check` and `render` each report it on the line of the
//! attribute with the sixteen names, exit 1, and `render` writes no film. Each of the sixteen names passes `check`.

mod common;

use std::fs;
use std::path::Path;

use common::{err, framesmith, out, project};

const GROUP: &str = "blend-reports";

const SIXTEEN: &str = "normal, multiply, screen, overlay, darken, lighten, color-dodge, color-burn, hard-light, soft-light, difference, exclusion, hue, saturation, color, luminosity";

fn composition(blend: &str) -> String {
    format!(
        "<composition version=\"1\" width=\"64\" height=\"48\" fps=\"10\" duration=\"0.5\" background=\"#000000\">\n\
         <box left=\"0\" top=\"0\" width=\"40\" height=\"40\" background=\"#336699\"/>\n\
         <box left=\"8\" top=\"8\" width=\"40\" height=\"20\" background=\"#cc3300\"\n     mix-blend-mode=\"{blend}\"/>\n\
         </composition>\n"
    )
}

fn has_a_film(project: &Path) -> bool {
    fs::read_dir(project.join("renders"))
        .map(|entries| {
            entries
                .flatten()
                .any(|entry| entry.path().extension().is_some_and(|ext| ext == "mp4"))
        })
        .unwrap_or(false)
}

#[test]
fn an_unknown_blend_name_is_reported_with_its_line_and_the_sixteen_names() {
    let (folder, cwd) = project(GROUP, "unknown", &composition("burn"));
    for command in ["check", "render"] {
        let output = framesmith(&cwd, [command, folder.to_str().unwrap()], &[]);
        let printed = format!("{}{}", out(&output), err(&output));
        assert_eq!(output.status.code(), Some(1), "{command}: {printed}");
        let wanted = format!("line 4: FS-0039: invalid: `mix-blend-mode=\"burn\"` is not one of: {SIXTEEN}");
        assert!(printed.contains(&wanted), "{command}: {wanted}\nin\n{printed}");
    }
    assert!(!has_a_film(&folder));
}

#[test]
fn each_of_the_sixteen_names_passes_check() {
    for name in SIXTEEN.split(", ") {
        let (folder, cwd) = project(GROUP, &format!("known-{name}"), &composition(name));
        let output = framesmith(&cwd, ["check", folder.to_str().unwrap()], &[]);
        assert_eq!(output.status.code(), Some(0), "{name}: {}{}", out(&output), err(&output));
    }
}
