//! T052: text is drawn with geometric precision unless the author says otherwise.

mod common;

use common::*;
use measure::Rgb;

const GREEN: Rgb = [0, 255, 0];
const BOXES: &str = ".box { position: absolute; top: 10px; width: 40px; height: 40px; background: #0000ff; }\n\
#root-box { left: 10px; } #word-box { left: 70px; } #own-box { left: 130px; }";
const BODY: &str = "<p id=\"word\">word</p><p id=\"own\">own</p>\n\
<div id=\"root-box\" class=\"box\"></div><div id=\"word-box\" class=\"box\"></div><div id=\"own-box\" class=\"box\"></div>";

/// Each box turns green when the element it stands for has the wanted text rendering, red otherwise.
fn script(root: &str, word: &str, own: &str) -> String {
    format!(
        "<script>\n\
function mark(box, element, wanted) {{\n\
  var has = getComputedStyle(element).textRendering.toLowerCase();\n\
  document.getElementById(box).style.background = has === wanted ? \"#00ff00\" : \"#ff0000\";\n\
}}\n\
mark(\"root-box\", document.documentElement, \"{root}\");\n\
mark(\"word-box\", document.getElementById(\"word\"), \"{word}\");\n\
mark(\"own-box\", document.getElementById(\"own\"), \"{own}\");\n\
</script>"
    )
}

fn greens(test: &str, style: &str, wanted: (&str, &str, &str)) -> Vec<bool> {
    let index = page(
        "data-fs-size=\"180x60\" data-fs-fps=\"10\" data-fs-duration=\"0.2\"",
        &format!("{BOXES}\n{style}"),
        BODY,
        &script(wanted.0, wanted.1, wanted.2),
    );
    let dir = project(test, "text", &index);
    let (ran, frames) = render_frames(&dir, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    [(30, 30), (90, 30), (150, 30)]
        .iter()
        .map(|&(x, y)| frames[1].pixel(x, y) == GREEN)
        .collect()
}

#[test]
fn t052_text_is_drawn_with_geometric_precision_and_an_element_with_its_own_value_keeps_it() {
    let seen = greens(
        "t052_text_default",
        "#own { text-rendering: optimizeSpeed; }",
        ("geometricprecision", "geometricprecision", "optimizespeed"),
    );
    assert_eq!(seen, [true, true, true]);
}

#[test]
fn t052_a_text_rendering_value_the_author_writes_on_the_root_wins() {
    let seen = greens("t052_text_author", "html { text-rendering: auto; }", ("auto", "auto", "auto"));
    assert_eq!(seen, [true, true, true]);
}
