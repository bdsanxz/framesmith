//! Slideshow manifest validation (Sprint 24, T001 / CAP-001).
//!
//! One `slideshow` node per project, slides referencing compositions
//! by id, with notes, fragments, branches, hotspots, and autoplay.
//! Every slide composition and every branch/hotspot target resolves
//! at validation, naming file, line, and ref on refusal. Empty decks
//! and zero/negative autoplay refuse with dedicated codes and also
//! emit lint findings `empty-deck` / `bad-autoplay`.

use crate::lint::LintFinding;
use crate::validate::{as_f64, as_str, find, find_either, raw_text, valid_id_syntax, Ctx};
use notation::model::Composition;
use notation::parse::RawNode;

/// Parse and validate one `slideshow` manifest. `compositions` are
/// the finished holdings so slide compositions resolve immediately;
/// `ctx.invalid` is set on refusal but the manifest is still returned
/// when usable for one more resolution pass (the caller re-resolves
/// refs after compositions are final).
pub fn validate_slideshow(
    ctx: &mut Ctx,
    roots: &[RawNode],
    compositions: &[Composition],
) -> Option<notation::model::SlideshowManifest> {
    let nodes: Vec<&RawNode> = roots.iter().filter(|n| n.name == "slideshow").collect();
    if nodes.is_empty() {
        return None;
    }
    if nodes.len() > 1 {
        let second = nodes[1];
        ctx.push(
            second.name_line,
            second.name_column,
            "E_BAD_SLIDESHOW",
            format!(
                "project holds {} `slideshow` nodes; a project has exactly one manifest",
                nodes.len()
            ),
            "keep a single `slideshow { slide ... }` block; remove the extra node",
        );
        return None;
    }
    let node = nodes[0];
    let autoplay = parse_autoplay(ctx, node);
    let mut seen_ids: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut slides: Vec<notation::model::SlideshowSlide> = Vec::new();
    let mut invalid = false;
    for (k, v) in &node.props {
        if !matches!(
            k.as_str(),
            "autoplay" | "autoplay_seconds" | "autoplay-seconds" | "auto_advance" | "auto-advance"
        ) {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("`slideshow` key {k:?} is not a slideshow property"),
                "use `autoplay=<positive seconds>` on `slideshow`",
            );
            invalid = true;
        }
    }
    for child in &node.children {
        match child.name.as_str() {
            "slide" => match parse_slide(ctx, child, &mut seen_ids) {
                Some(s) => slides.push(s),
                None => invalid = true,
            },
            other => {
                ctx.push(
                    child.name_line,
                    child.name_column,
                    "E_BAD_SLIDESHOW",
                    format!("`slideshow` child {other:?} is not a slide; slides reference compositions by id and never embed them"),
                    "nest only `slide` nodes inside `slideshow`",
                );
                invalid = true;
            }
        }
    }
    if slides.is_empty() {
        ctx.push(
            node.name_line,
            node.name_column,
            "E_EMPTY_DECK",
            "slideshow declares no slides; an empty deck never presents and never renders a slide",
            "add at least one `slide id=\"s1\" composition=\"...\"` inside `slideshow`",
        );
        if let Some(f) = LintFinding::new(
            "empty-deck",
            &ctx.file.clone(),
            node.name_line,
            node.name_column,
            "slideshow declares no slides; present has nothing to show",
            "add at least one `slide id=\"...\" composition=\"...\"` inside `slideshow`",
        ) {
            ctx.lint_findings.push(f);
        }
        invalid = true;
    }
    // Bad autoplay already pushed E_BAD_AUTOPLAY + lint; absence is manual.
    if invalid {
        return None;
    }
    let manifest = notation::model::SlideshowManifest {
        slides: slides.clone(),
        autoplay,
        line: node.name_line,
        column: node.name_column,
    };
    validate_refs(ctx, &manifest, compositions);
    if ctx.invalid {
        return None;
    }
    Some(manifest)
}

fn parse_autoplay(ctx: &mut Ctx, node: &RawNode) -> Option<f64> {
    let v = find(node, "autoplay")
        .or_else(|| find(node, "autoplay_seconds"))
        .or_else(|| find(node, "autoplay-seconds"))
        .or_else(|| find(node, "auto_advance"))
        .or_else(|| find(node, "auto-advance"));
    match v {
        None => None,
        Some(val) => match as_f64(val) {
            Some(x) if x.is_finite() && x > 0.0 => Some(x),
            _ => {
                ctx.push(
                    val.line,
                    val.column,
                    "E_BAD_AUTOPLAY",
                    format!(
                        "`slideshow` autoplay {} must be a positive number of seconds; zero, negative, and non-finite durations never auto-advance",
                        raw_text(val)
                    ),
                    "set `autoplay=<positive seconds>` (for example `autoplay=5`), or remove the key for manual advance only",
                );
                if let Some(f) = LintFinding::new(
                    "bad-autoplay",
                    &ctx.file.clone(),
                    val.line,
                    val.column,
                    "slideshow autoplay is zero, negative, or non-finite; the deck never auto-advances",
                    "set `autoplay=<positive seconds>` or remove the key for manual advance only",
                ) {
                    ctx.lint_findings.push(f);
                }
                None
            }
        },
    }
}

fn parse_slide(
    ctx: &mut Ctx,
    node: &RawNode,
    seen_ids: &mut std::collections::BTreeSet<String>,
) -> Option<notation::model::SlideshowSlide> {
    let id = match find(node, "id").and_then(as_str) {
        Some(s) if valid_id_syntax(s) => s.to_string(),
        _ => {
            let (line, col) = find(node, "id")
                .map(|v| (v.line, v.column))
                .unwrap_or((node.name_line, node.name_column));
            ctx.push(
                line,
                col,
                "E_BAD_SLIDESHOW",
                format!(
                    "slide `id` must be 1-64 letters, digits, `-` or `_`; found {}",
                    find(node, "id").map(raw_text).unwrap_or_else(|| "absent".into())
                ),
                "set `id=\"s1\"` on every `slide`",
            );
            return None;
        }
    };
    if !seen_ids.insert(id.clone()) {
        ctx.push(
            node.name_line,
            node.name_column,
            "E_BAD_SLIDESHOW",
            format!("duplicate slide id {id:?}; slide ids are unique like composition ids"),
            format!("rename one of the `id=\"{id}\"` slides to a unique id"),
        );
        return None;
    }
    let composition = match find(node, "composition").and_then(as_str) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            let (line, col) = find(node, "composition")
                .map(|v| (v.line, v.column))
                .unwrap_or((node.name_line, node.name_column));
            ctx.push(
                line,
                col,
                "E_BAD_SLIDESHOW",
                format!(
                    "slide {id:?} `composition` must name a composition; found {}",
                    find(node, "composition").map(raw_text).unwrap_or_else(|| "absent".into())
                ),
                format!(
                    "set `composition=\"<composition name>\"` on slide {id:?}; slides reference, never embed, compositions"
                ),
            );
            return None;
        }
    };
    let mut notes: Option<String> = match find(node, "notes") {
        None => None,
        Some(v) => match as_str(v) {
            Some(s) => Some(s.to_string()),
            None => {
                ctx.push(
                    v.line,
                    v.column,
                    "E_BAD_SLIDESHOW",
                    format!("slide {id:?} `notes` must be a quoted string; found {}", raw_text(v)),
                    "set `notes=\"...\"` or remove the key",
                );
                return None;
            }
        },
    };
    for (k, v) in &node.props {
        if !["id", "composition", "notes"].contains(&k.as_str()) {
            ctx.push(
                v.line,
                v.column,
                "E_UNKNOWN_KEY",
                format!("`slide` key {k:?} is not a slide property"),
                "use `id`, `composition`, and `notes` on `slide`",
            );
            return None;
        }
    }
    let mut fragments = Vec::new();
    let mut branches = Vec::new();
    let mut hotspots = Vec::new();
    for child in &node.children {
        match child.name.as_str() {
            "fragment" => {
                let at = match find(child, "at").and_then(as_f64) {
                    Some(x) if x.is_finite() && x >= 0.0 => x,
                    _ => {
                        let (line, col) = find(child, "at")
                            .map(|v| (v.line, v.column))
                            .unwrap_or((child.name_line, child.name_column));
                        ctx.push(
                            line,
                            col,
                            "E_BAD_SLIDESHOW",
                            format!(
                                "fragment `at` must be a finite hold time in seconds; found {}",
                                find(child, "at").map(raw_text).unwrap_or_else(|| "absent".into())
                            ),
                            "set `fragment at=<seconds>`",
                        );
                        return None;
                    }
                };
                let frag_id = find(child, "id").and_then(as_str).map(|s| s.to_string());
                for (k, v) in &child.props {
                    if !["at", "id"].contains(&k.as_str()) {
                        ctx.push(
                            v.line,
                            v.column,
                            "E_UNKNOWN_KEY",
                            format!("`fragment` key {k:?} is not a fragment property"),
                            "use `at` and optional `id` on `fragment`",
                        );
                        return None;
                    }
                }
                fragments.push(notation::model::SlideshowFragment {
                    id: frag_id,
                    at,
                    line: child.name_line,
                    column: child.name_column,
                });
            }
            "notes" => {
                if notes.is_some() {
                    ctx.push(
                        child.name_line,
                        child.name_column,
                        "E_BAD_SLIDESHOW",
                        format!("slide {id:?} declares notes twice"),
                        "keep one `notes` per slide",
                    );
                    return None;
                }
                match find(child, "value").and_then(as_str) {
                    Some(s) => notes = Some(s.to_string()),
                    None => {
                        ctx.push(
                            child.name_line,
                            child.name_column,
                            "E_BAD_SLIDESHOW",
                            "notes child needs `value=\"...\"`".to_string(),
                            "write `notes value=\"...\"`",
                        );
                        return None;
                    }
                }
            }
            "branch" => {
                let to = match find(child, "to").and_then(as_str) {
                    Some(s) if valid_id_syntax(s) => s.to_string(),
                    _ => {
                        let (line, col) = find(child, "to")
                            .map(|v| (v.line, v.column))
                            .unwrap_or((child.name_line, child.name_column));
                        ctx.push(
                            line,
                            col,
                            "E_BAD_SLIDESHOW",
                            format!(
                                "branch `to` must name a slide id; found {}",
                                find(child, "to").map(raw_text).unwrap_or_else(|| "absent".into())
                            ),
                            "set `branch to=\"<slide id>\"`",
                        );
                        return None;
                    }
                };
                for (k, v) in &child.props {
                    if k != "to" {
                        ctx.push(
                            v.line,
                            v.column,
                            "E_UNKNOWN_KEY",
                            format!("`branch` key {k:?} is not a branch property"),
                            "use `to` on `branch`",
                        );
                        return None;
                    }
                }
                branches.push(notation::model::SlideshowBranch {
                    to,
                    line: child.name_line,
                    column: child.name_column,
                });
            }
            "hotspot" => {
                let hs_id = match find(child, "id").and_then(as_str) {
                    Some(s) if !s.is_empty() => s.to_string(),
                    _ => {
                        let (line, col) = find(child, "id")
                            .map(|v| (v.line, v.column))
                            .unwrap_or((child.name_line, child.name_column));
                        ctx.push(
                            line,
                            col,
                            "E_BAD_SLIDESHOW",
                            format!(
                                "hotspot `id` is required; found {}",
                                find(child, "id").map(raw_text).unwrap_or_else(|| "absent".into())
                            ),
                            "set `hotspot id=\"h1\" x=0 y=0 target=\"s2\"`",
                        );
                        return None;
                    }
                };
                let x = match find_either(child, "x", "x").and_then(as_f64) {
                    Some(v) if v.is_finite() => v,
                    _ => {
                        let (line, col) = find(child, "x")
                            .map(|v| (v.line, v.column))
                            .unwrap_or((child.name_line, child.name_column));
                        ctx.push(
                            line,
                            col,
                            "E_BAD_SLIDESHOW",
                            format!(
                                "hotspot {hs_id:?} `x` must be a number; found {}",
                                find(child, "x").map(raw_text).unwrap_or_else(|| "absent".into())
                            ),
                            "set `x=<pixels>`",
                        );
                        return None;
                    }
                };
                let y = match find(child, "y").and_then(as_f64) {
                    Some(v) if v.is_finite() => v,
                    _ => {
                        let (line, col) = find(child, "y")
                            .map(|v| (v.line, v.column))
                            .unwrap_or((child.name_line, child.name_column));
                        ctx.push(
                            line,
                            col,
                            "E_BAD_SLIDESHOW",
                            format!(
                                "hotspot {hs_id:?} `y` must be a number; found {}",
                                find(child, "y").map(raw_text).unwrap_or_else(|| "absent".into())
                            ),
                            "set `y=<pixels>`",
                        );
                        return None;
                    }
                };
                let target = match find(child, "target").and_then(as_str) {
                    Some(s) if valid_id_syntax(s) => s.to_string(),
                    _ => {
                        let (line, col) = find(child, "target")
                            .map(|v| (v.line, v.column))
                            .unwrap_or((child.name_line, child.name_column));
                        ctx.push(
                            line,
                            col,
                            "E_BAD_SLIDESHOW",
                            format!(
                                "hotspot {hs_id:?} `target` must name a slide id; found {}",
                                find(child, "target").map(raw_text).unwrap_or_else(|| "absent".into())
                            ),
                            "set `target=\"<slide id>\"`",
                        );
                        return None;
                    }
                };
                for (k, v) in &child.props {
                    if !["id", "x", "y", "target"].contains(&k.as_str()) {
                        ctx.push(
                            v.line,
                            v.column,
                            "E_UNKNOWN_KEY",
                            format!("`hotspot` key {k:?} is not a hotspot property"),
                            "use `id`, `x`, `y`, and `target` on `hotspot`",
                        );
                        return None;
                    }
                }
                hotspots.push(notation::model::SlideshowHotspot {
                    id: hs_id,
                    x,
                    y,
                    target,
                    line: child.name_line,
                    column: child.name_column,
                });
            }
            other => {
                if other == "composition" {
                    ctx.push(
                        child.name_line,
                        child.name_column,
                        "E_BAD_SLIDESHOW",
                        format!(
                            "slides reference compositions by id and never embed them; found a nested `composition` inside slide {id:?}"
                        ),
                        "move the composition to the top level and point the slide at it with `composition=\"...\"`",
                    );
                } else {
                    ctx.push(
                        child.name_line,
                        child.name_column,
                        "E_BAD_SLIDESHOW",
                        format!(
                            "`slide` child {other:?} is not allowed; expected `fragment`, `branch`, `hotspot`, or `notes`"
                        ),
                        "nest only `fragment at=`, `branch to=`, `hotspot id=`, or `notes value=` inside `slide`",
                    );
                }
                return None;
            }
        }
    }
    Some(notation::model::SlideshowSlide {
        id,
        composition,
        notes,
        fragments,
        branches,
        hotspots,
        line: node.name_line,
        column: node.name_column,
    })
}

fn validate_refs(
    ctx: &mut Ctx,
    deck: &notation::model::SlideshowManifest,
    compositions: &[Composition],
) {
    use std::collections::BTreeSet;
    let comp_names: BTreeSet<String> = compositions.iter().map(|c| c.name.clone()).collect();
    let comp_ids: BTreeSet<String> = compositions.iter().filter_map(|c| c.id.clone()).collect();
    let slide_ids: BTreeSet<String> = deck.slides.iter().map(|s| s.id.clone()).collect();
    for slide in &deck.slides {
        if !comp_names.contains(&slide.composition) && !comp_ids.contains(&slide.composition) {
            ctx.push(
                slide.line,
                slide.column,
                "E_UNKNOWN_REF",
                format!(
                    "slide {:?} references unknown composition {:?}; the project holds {}",
                    slide.id,
                    slide.composition,
                    compositions.iter().map(|c| format!("{:?}", c.name)).collect::<Vec<_>>().join(", ")
                ),
                format!(
                    "set `composition` on slide {:?} to one of the declared composition names, or add that composition",
                    slide.id
                ),
            );
        }
        for b in &slide.branches {
            if !slide_ids.contains(&b.to) {
                ctx.push(
                    b.line,
                    b.column,
                    "E_UNKNOWN_REF",
                    format!(
                        "slide {:?} branch targets unknown slide {:?}; the deck holds {}",
                        slide.id,
                        b.to,
                        deck.slides.iter().map(|s| format!("{:?}", s.id)).collect::<Vec<_>>().join(", ")
                    ),
                    format!("point the branch at a declared slide id, or add a slide with `id=\"{}\"`", b.to),
                );
            }
        }
        for h in &slide.hotspots {
            if !slide_ids.contains(&h.target) {
                ctx.push(
                    h.line,
                    h.column,
                    "E_UNKNOWN_REF",
                    format!(
                        "slide {:?} hotspot {:?} targets unknown slide {:?}; the deck holds {}",
                        slide.id,
                        h.id,
                        h.target,
                        deck.slides.iter().map(|s| format!("{:?}", s.id)).collect::<Vec<_>>().join(", ")
                    ),
                    format!(
                        "point the hotspot at a declared slide id, or add a slide with `id=\"{}\"`",
                        h.target
                    ),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static N: AtomicU64 = AtomicU64::new(0);

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "framesmith-slideshow-{}-{}-{}",
            tag,
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_comp(dir: &std::path::Path, body: &str) {
        fs::write(dir.join("composition.kdl"), body).unwrap();
    }

    #[test]
    fn valid_manifest_resolves() {
        let dir = tmp("valid");
        write_comp(
            &dir,
            "composition name=\"intro\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1\n}\ncomposition name=\"agenda\" root=true width=64 height=64 duration=1 background=\"#000\" version=1 {\n box x=0 y=0 width=64 height=64 fill=\"#00ff00\" start=0 duration=1\n}\nslideshow autoplay=5 {\n slide id=\"s1\" composition=\"intro\" notes=\"welcome\" {\n  fragment at=0.5 id=\"f1\"\n  branch to=\"s2\"\n  hotspot id=\"h1\" x=10 y=10 target=\"s2\"\n }\n slide id=\"s2\" composition=\"agenda\" {\n  notes value=\"agenda notes\"\n }\n}\n",
        );
        let v = crate::validate::validate_folder(&dir);
        assert_eq!(v.status, crate::validate::Status::Valid, "{:?}", v.diagnostics);
        let deck = v.slideshow.expect("valid deck");
        assert_eq!(deck.autoplay, Some(5.0));
        assert_eq!(deck.slides.len(), 2);
        assert_eq!(deck.slides[0].id, "s1");
        assert_eq!(deck.slides[0].notes.as_deref(), Some("welcome"));
        assert_eq!(deck.slides[0].fragments.len(), 1);
        assert_eq!(deck.slides[0].branches[0].to, "s2");
        assert_eq!(deck.slides[0].hotspots[0].target, "s2");
    }

    #[test]
    fn empty_deck_refuses() {
        let dir = tmp("empty");
        write_comp(
            &dir,
            "composition name=\"solo\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1\n}\nslideshow {\n}\n",
        );
        let v = crate::validate::validate_folder(&dir);
        assert_eq!(v.status, crate::validate::Status::Invalid);
        assert!(v.diagnostics.iter().any(|d| d.code == "E_EMPTY_DECK"), "{:?}", v.diagnostics);
        assert!(v.lint_findings.iter().any(|f| f.rule == "empty-deck"), "{:?}", v.lint_findings);
        assert!(v.slideshow.is_none());
    }

    #[test]
    fn bad_autoplay_refuses() {
        let dir = tmp("auto");
        for val in ["0", "-1", "\"fast\""] {
            write_comp(
                &dir,
                &format!(
                    "composition name=\"solo\" width=64 height=64 duration=1 background=\"#000\" version=1 {{\n box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1\n}}\nslideshow autoplay={val} {{\n slide id=\"s1\" composition=\"solo\"\n}}\n"
                ),
            );
            let v = crate::validate::validate_folder(&dir);
            assert_eq!(v.status, crate::validate::Status::Invalid, "{val}: {:?}", v.diagnostics);
            assert!(v.diagnostics.iter().any(|d| d.code == "E_BAD_AUTOPLAY"), "{val}: {:?}", v.diagnostics);
        }
    }

    #[test]
    fn unknown_composition_refuses() {
        let dir = tmp("unknown-comp");
        write_comp(
            &dir,
            "composition name=\"solo\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1\n}\nslideshow {\n slide id=\"s1\" composition=\"ghost\"\n}\n",
        );
        let v = crate::validate::validate_folder(&dir);
        assert_eq!(v.status, crate::validate::Status::Invalid);
        assert!(v.diagnostics.iter().any(|d| d.code == "E_UNKNOWN_REF" && d.message.contains("ghost")), "{:?}", v.diagnostics);
    }

    #[test]
    fn broken_branch_refuses() {
        let dir = tmp("broken-branch");
        write_comp(
            &dir,
            "composition name=\"solo\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1\n}\nslideshow {\n slide id=\"s1\" composition=\"solo\" {\n  branch to=\"missing\"\n }\n}\n",
        );
        let v = crate::validate::validate_folder(&dir);
        assert_eq!(v.status, crate::validate::Status::Invalid);
        assert!(v.diagnostics.iter().any(|d| d.code == "E_UNKNOWN_REF" && d.message.contains("missing")), "{:?}", v.diagnostics);
    }

    #[test]
    fn broken_hotspot_refuses() {
        let dir = tmp("broken-hot");
        write_comp(
            &dir,
            "composition name=\"solo\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1\n}\nslideshow {\n slide id=\"s1\" composition=\"solo\" {\n  hotspot id=\"h1\" x=0 y=0 target=\"gone\"\n }\n}\n",
        );
        let v = crate::validate::validate_folder(&dir);
        assert_eq!(v.status, crate::validate::Status::Invalid);
        assert!(v.diagnostics.iter().any(|d| d.code == "E_UNKNOWN_REF" && d.message.contains("gone")), "{:?}", v.diagnostics);
    }

    #[test]
    fn multiple_manifests_refuse() {
        let dir = tmp("multi");
        write_comp(
            &dir,
            "composition name=\"solo\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1\n}\nslideshow {\n slide id=\"s1\" composition=\"solo\"\n}\nslideshow {\n slide id=\"s2\" composition=\"solo\"\n}\n",
        );
        let v = crate::validate::validate_folder(&dir);
        assert_eq!(v.status, crate::validate::Status::Invalid);
        assert!(v.diagnostics.iter().any(|d| d.code == "E_BAD_SLIDESHOW"), "{:?}", v.diagnostics);
    }

    #[test]
    fn embedded_composition_refuses() {
        let dir = tmp("embedded");
        write_comp(
            &dir,
            "composition name=\"solo\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n box x=0 y=0 width=64 height=64 fill=\"#ff0000\" start=0 duration=1\n}\nslideshow {\n slide id=\"s1\" composition=\"solo\" {\n  composition name=\"nested\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n  }\n }\n}\n",
        );
        let v = crate::validate::validate_folder(&dir);
        assert_eq!(v.status, crate::validate::Status::Invalid);
        assert!(v.diagnostics.iter().any(|d| d.code == "E_BAD_SLIDESHOW" && d.message.contains("never embed")), "{:?}", v.diagnostics);
    }
}
