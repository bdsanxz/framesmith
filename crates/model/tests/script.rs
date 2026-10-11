use model::script::scan;

fn found(text: &str) -> Vec<(&'static str, &'static str, u32)> {
    scan(text).into_iter().map(|u| (u.code, u.what, u.line)).collect()
}

#[test]
fn t031_the_browser_animation_interface_is_found() {
    assert_eq!(
        found("const box = document.querySelector('#a');\nbox.animate([{opacity: 0}], 1000);"),
        vec![("FS-U016", ".animate(", 2)]
    );
    assert_eq!(found("el\n  .animate (frames)"), vec![("FS-U016", ".animate(", 2)]);
    assert_eq!(found("new Animation(effect)"), vec![("FS-U016", "new Animation", 1)]);
    assert_eq!(
        found("const e = new KeyframeEffect(a, b, c);"),
        vec![("FS-U016", "KeyframeEffect", 1)]
    );
    assert_eq!(
        found("\n\ndocument.getAnimations().forEach(stop)"),
        vec![("FS-U016", "getAnimations", 3)]
    );
    assert_eq!(
        found("const t = document.timeline.currentTime"),
        vec![("FS-U016", "document.timeline", 1)]
    );
}

#[test]
fn t031_the_script_interface_is_found() {
    assert_eq!(found("framesmith.timeline().to('#a', {x: 1})"), vec![("FS-U017", "framesmith", 1)]);
    assert_eq!(found("\nconst t = window.framesmith;"), vec![("FS-U017", "framesmith", 2)]);
    assert_eq!(found("globalThis?.framesmith"), vec![("FS-U017", "framesmith", 1)]);
}

#[test]
fn comments_strings_and_other_names_are_not_uses() {
    for clean in [
        "// box.animate(x) and framesmith",
        "/* new Animation()\n framesmith */",
        "const s = \"framesmith.animate(\" + 'KeyframeEffect' + `getAnimations`;",
        "const myAnimation = 1; const Animations = 2; isAnimation(x)",
        "app.framesmith = 1; my.timeline.play(); animate(x); obj.animated(1)",
        "const __framesmith = 1; framesmithy()",
        "const timeline = [];",
    ] {
        assert_eq!(found(clean), vec![], "{clean}");
    }
}

#[test]
fn every_line_with_a_use_is_reported_once() {
    let uses = found("a.animate(x); b.animate(y);\nc.animate(z)");
    assert_eq!(uses, vec![("FS-U016", ".animate(", 1), ("FS-U016", ".animate(", 2)]);
}
