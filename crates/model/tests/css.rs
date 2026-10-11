use model::css::{plain_value, scan};

#[test]
fn declarations_carry_their_lines() {
    let sheet = scan(
        "/* one\n   two */\n.a {\n  color: red;\n  ANIMATION: spin 2s infinite\n}\n.b:hover { transition : opacity .2s !important; }\n",
    );
    let found: Vec<(&str, &str, u32)> = sheet
        .declarations
        .iter()
        .map(|d| (d.property.as_str(), d.value.as_str(), d.line))
        .collect();
    assert_eq!(
        found,
        vec![
            ("color", "red", 4),
            ("animation", "spin 2s infinite", 5),
            ("transition", "opacity .2s !important", 7)
        ]
    );
    assert_eq!(plain_value("Opacity .2s  !IMPORTANT"), "opacity .2s");
    assert_eq!(plain_value(" NONE "), "none");
}

#[test]
fn at_rules_are_listed() {
    let sheet = scan("@charset \"utf-8\";\n@media (min-width: 10px) {\n  @keyframes spin { from { rotate: 0deg } to { rotate: 360deg } }\n}\n@font-face { font-family: \"Ours\"; src: url(fonts/ours.woff2) format(\"woff2\"); }");
    let rules: Vec<(&str, &str, u32)> = sheet
        .at_rules
        .iter()
        .map(|r| (r.name.as_str(), r.prelude.as_str(), r.line))
        .collect();
    assert_eq!(
        rules,
        vec![
            ("charset", "\"utf-8\"", 1),
            ("media", "(min-width: 10px)", 2),
            ("keyframes", "spin", 3),
            ("font-face", "", 5)
        ]
    );
}

#[test]
fn references_are_found_with_their_lines() {
    let sheet = scan("@import \"base.css\";\n@import url('more/theme.css') screen;\n.a { background: URL( \"art/bg.png\" ) no-repeat,\n   url(data:image/png;base64,AAAA;BBBB) }\n.b { content: \"url(inside-a-string.png)\"; mask: url(#clip) }\n.c { x: myurl(nope.png) }");
    let urls: Vec<(&str, u32, bool)> = sheet.urls.iter().map(|u| (u.url.as_str(), u.line, u.import)).collect();
    assert_eq!(
        urls,
        vec![
            ("base.css", 1, true),
            ("more/theme.css", 2, true),
            ("art/bg.png", 3, false),
            ("data:image/png;base64,AAAA;BBBB", 4, false),
            ("#clip", 5, false),
        ]
    );
}

#[test]
fn a_style_attribute_is_a_list_of_declarations() {
    let sheet = scan("translate: 10px 0; opacity:.5;background:url(a.png)");
    let found: Vec<(&str, &str)> = sheet.declarations.iter().map(|d| (d.property.as_str(), d.value.as_str())).collect();
    assert_eq!(
        found,
        vec![("translate", "10px 0"), ("opacity", ".5"), ("background", "url(a.png)")]
    );
    assert_eq!(sheet.urls.len(), 1);
}

#[test]
fn odd_input_never_stops_the_scanner() {
    for text in [
        "",
        "{",
        "}",
        "a{b:c",
        "/* open",
        "\"open",
        "@",
        "a:b;;;",
        "x: url(",
        "é { ü: \"ñ\" }",
    ] {
        let _ = scan(text);
    }
}
