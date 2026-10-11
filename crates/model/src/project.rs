//! `check`: reads a project folder and reports everything a render would
//! trip over, without starting a browser.

use crate::html::{self, Dom, Element};
use crate::{css, exit_code, image, script, selector};
use crate::{Diagnostic, Draft, Manifest, RawAttr, RawDoc, RawElement, RawMotion, MOTION_TYPE};
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

/// The entry page of a project.
pub const ENTRY_NAME: &str = "index.html";
/// The path the loader and the runtime are served from. Projects may not use it.
pub const RESERVED_PREFIX: &str = "/__framesmith__/";

/// How deep `@import` is followed.
const MAX_IMPORT_DEPTH: usize = 8;

/// What `check` found.
#[derive(Clone, Debug, Default)]
pub struct CheckReport {
    /// Sorted by file and line, the entry page first.
    pub diagnostics: Vec<Diagnostic>,
    /// Present when the composition, its clips and its tweens resolved.
    pub manifest: Option<Manifest>,
}

impl CheckReport {
    /// 0 when a render would proceed, 1 for any error, 2 when the only
    /// blockers are unsupported or unknown uses.
    pub fn exit_code(&self) -> i32 {
        exit_code(&self.diagnostics)
    }
}

/// Where `render` writes when no output path is given: `out/<folder name>.mp4`
/// inside the project.
pub fn default_output(project: &Path) -> PathBuf {
    let name = std::fs::canonicalize(project)
        .ok()
        .and_then(|path| path.file_name().map(|name| name.to_string_lossy().into_owned()))
        .or_else(|| project.file_name().map(|name| name.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "output".to_string());
    project.join("out").join(format!("{name}.mp4"))
}

/// What a reference written in a page or a style sheet points at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reference {
    /// Nothing to load: empty, a fragment, or data written in place.
    Nothing,
    /// A network address.
    Network,
    /// Outside the project folder.
    Outside,
    /// Inside the reserved path.
    Reserved,
    /// A file in the project, as a path from the project folder.
    Local(PathBuf),
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut pos = 0;
    while pos < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match (
            bytes[pos],
            bytes.get(pos + 1).copied().and_then(hex),
            bytes.get(pos + 2).copied().and_then(hex),
        ) {
            (b'%', Some(high), Some(low)) => {
                out.push((high * 16 + low) as u8);
                pos += 3;
            }
            (b, _, _) => {
                out.push(b);
                pos += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Works out what `reference` points at when written in a file in `base`
/// (a folder, as a path from the project folder).
pub fn classify(reference: &str, base: &Path) -> Reference {
    let reference = reference.trim();
    if reference.is_empty() || reference.starts_with('#') {
        return Reference::Nothing;
    }
    let lower = reference.to_ascii_lowercase();
    if lower.starts_with("//") || lower.starts_with("http:") || lower.starts_with("https:") {
        return Reference::Network;
    }
    let scheme_end = lower
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-')))
        .unwrap_or(lower.len());
    if scheme_end > 1 && lower[scheme_end..].starts_with(':') {
        return match &lower[..scheme_end] {
            "data" | "blob" | "about" | "javascript" => Reference::Nothing,
            _ => Reference::Outside,
        };
    }
    let path = reference.split(['?', '#']).next().unwrap_or("");
    let path = percent_decode(path);
    let (mut parts, path): (Vec<String>, &str) = match path.strip_prefix('/') {
        Some(rest) => (Vec::new(), rest),
        None => (
            base.components()
                .filter_map(|c| match c {
                    Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
                    _ => None,
                })
                .collect(),
            path.as_str(),
        ),
    };
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Reference::Outside;
                }
            }
            part => parts.push(part.to_string()),
        }
    }
    let reserved = RESERVED_PREFIX.trim_matches('/');
    if parts.first().is_some_and(|first| first == reserved) {
        return Reference::Reserved;
    }
    if parts.is_empty() {
        return Reference::Nothing;
    }
    Reference::Local(parts.iter().collect())
}

/// Picks out of a page what the composition is resolved from. Also returns,
/// for each raw element, its index in the page.
pub fn raw_doc(dom: &Dom) -> (RawDoc, Vec<usize>) {
    let mut raw = RawDoc::default();
    let mut page_index: Vec<usize> = Vec::new();
    // Page index to raw index, for elements that are in the raw list.
    let mut raw_index: Vec<Option<usize>> = vec![None; dom.elements.len()];
    for (index, element) in dom.elements.iter().enumerate() {
        if element.tag == "script" && is_motion_block(element) {
            raw.motion.push(RawMotion {
                text: element.text.clone(),
                line: element.text_line,
            });
        }
        let attrs: Vec<RawAttr> = element
            .attrs
            .iter()
            .filter(|attr| attr.name.starts_with("data-fs-"))
            .map(|attr| RawAttr {
                name: attr.name.clone(),
                value: attr.value.clone(),
                line: attr.line,
            })
            .collect();
        if attrs.is_empty() {
            continue;
        }
        let mut parent = None;
        let mut ancestor = element.parent;
        while let Some(a) = ancestor {
            if let Some(found) = raw_index[a] {
                parent = Some(found);
                break;
            }
            ancestor = dom.elements[a].parent;
        }
        raw_index[index] = Some(raw.elements.len());
        page_index.push(index);
        raw.elements.push(RawElement {
            tag: element.tag.clone(),
            id: element.value("id").map(str::to_string),
            attrs,
            parent,
            line: element.line,
        });
    }
    (raw, page_index)
}

fn script_type(element: &Element) -> String {
    element.value("type").unwrap_or("").trim().to_ascii_lowercase()
}

fn is_motion_block(element: &Element) -> bool {
    script_type(element) == MOTION_TYPE
}

fn is_page_script(element: &Element) -> bool {
    matches!(
        script_type(element).as_str(),
        "" | "module" | "text/javascript" | "application/javascript" | "text/ecmascript" | "application/ecmascript"
    )
}

fn display(path: &Path) -> String {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

struct Scan<'a> {
    root: &'a Path,
    diagnostics: Vec<Diagnostic>,
    sheets_seen: BTreeSet<PathBuf>,
}

impl Scan<'_> {
    fn add(&mut self, code: &str, file: &str, line: u32, description: impl Into<String>, remedy: impl Into<String>) {
        self.diagnostics.push(Diagnostic::new(code, file, line, description, remedy));
    }

    /// Checks one reference. Returns the file when it is in the project and exists.
    fn reference(&mut self, file: &str, line: u32, base: &Path, url: &str, what: &str) -> Option<PathBuf> {
        match classify(url, base) {
            Reference::Nothing => None,
            Reference::Network => {
                self.add(
                    "FS-U009",
                    file,
                    line,
                    format!("The {what} \"{url}\" is loaded from a network address; network assets are not supported in this version."),
                    "Download the file into the project folder and refer to the local copy.",
                );
                None
            }
            Reference::Outside => {
                self.add(
                    "FS-E031",
                    file,
                    line,
                    format!("The {what} \"{url}\" points outside the project folder; assets must be inside the project."),
                    "Copy the file into the project folder and refer to it there.",
                );
                None
            }
            Reference::Reserved => {
                self.add(
                    "FS-E031",
                    file,
                    line,
                    format!("The {what} \"{url}\" points into {RESERVED_PREFIX}, a path Framesmith keeps for itself."),
                    "Move the file to another folder in the project and refer to it there.",
                );
                None
            }
            Reference::Local(path) => {
                let full = self.root.join(&path);
                if !full.is_file() {
                    self.add(
                        "FS-E030",
                        file,
                        line,
                        format!(
                            "The {what} \"{url}\" does not exist ({} was looked for in the project folder).",
                            display(&path)
                        ),
                        "Add the file to the project, or correct the reference.",
                    );
                    return None;
                }
                // A link inside the project may still lead out of it.
                let inside = match (std::fs::canonicalize(&full), std::fs::canonicalize(self.root)) {
                    (Ok(full), Ok(root)) => full.starts_with(root),
                    _ => true,
                };
                if !inside {
                    self.add(
                        "FS-E031",
                        file,
                        line,
                        format!("The {what} \"{url}\" is a link that leads outside the project folder."),
                        "Copy the file itself into the project folder.",
                    );
                    return None;
                }
                Some(path)
            }
        }
    }

    /// Checks a reference to a picture, and that the picture is a still one.
    fn picture(&mut self, file: &str, line: u32, base: &Path, url: &str, what: &str) {
        let Some(path) = self.reference(file, line, base, url, what) else {
            return;
        };
        let animated = std::fs::read(self.root.join(&path)).is_ok_and(|bytes| image::is_animated(&bytes));
        if animated {
            self.add(
                "FS-U008",
                file,
                line,
                format!("The {what} \"{url}\" is an animated image; animated images are not supported in this version."),
                "Use a still image, for example one frame exported as PNG.",
            );
        }
    }

    /// Scans CSS written in `file`. `first_line` is the line its text starts
    /// on; a style attribute passes `whole_on_line` so every finding gets
    /// that one line.
    fn style(&mut self, file: &str, first_line: u32, whole_on_line: bool, base: &Path, text: &str, depth: usize) {
        let sheet = css::scan(text);
        let at = |line: u32| if whole_on_line { first_line } else { first_line + line - 1 };
        for rule in &sheet.at_rules {
            if rule.name == "keyframes" || rule.name.ends_with("-keyframes") {
                self.add(
                    "FS-U014",
                    file,
                    at(rule.line),
                    format!(
                        "The rule @{} {} declares a CSS animation; CSS animations are not supported in this version.",
                        rule.name, rule.prelude
                    ),
                    "Remove the rule and animate with a motion block instead.",
                );
            }
        }
        for declaration in &sheet.declarations {
            let property = declaration.property.trim_start_matches("-webkit-").trim_start_matches("-moz-");
            let value = css::plain_value(&declaration.value);
            match property {
                "animation" | "animation-name" if value != "none" => self.add(
                    "FS-U014",
                    file,
                    at(declaration.line),
                    format!(
                        "The declaration \"{}: {}\" starts a CSS animation; CSS animations are not supported in this version.",
                        declaration.property, declaration.value
                    ),
                    "Remove the declaration and animate with a motion block instead.",
                ),
                "transition" if value != "none" => self.add(
                    "FS-U015",
                    file,
                    at(declaration.line),
                    format!(
                        "The declaration \"{}: {}\" sets a CSS transition; CSS transitions are not supported in this version.",
                        declaration.property, declaration.value
                    ),
                    "Remove the declaration and animate with a motion block instead.",
                ),
                "transition-duration" if is_nonzero_time(&value) => self.add(
                    "FS-U015",
                    file,
                    at(declaration.line),
                    format!(
                        "The declaration \"{}: {}\" sets a CSS transition; CSS transitions are not supported in this version.",
                        declaration.property, declaration.value
                    ),
                    "Remove the declaration and animate with a motion block instead.",
                ),
                _ => {}
            }
        }
        for url in &sheet.urls {
            if url.import {
                if let Some(path) = self.reference(file, at(url.line), base, &url.url, "style sheet") {
                    self.style_file(&path, depth + 1);
                }
            } else {
                self.picture(file, at(url.line), base, &url.url, "file");
            }
        }
    }

    fn style_file(&mut self, path: &Path, depth: usize) {
        if depth > MAX_IMPORT_DEPTH || !self.sheets_seen.insert(path.to_path_buf()) {
            return;
        }
        let Ok(bytes) = std::fs::read(self.root.join(path)) else {
            return;
        };
        let text = String::from_utf8_lossy(&bytes);
        let base = path.parent().unwrap_or(Path::new("")).to_path_buf();
        self.style(&display(path), 1, false, &base, &text, depth);
    }

    fn script(&mut self, file: &str, first_line: u32, text: &str) {
        for found in script::scan(text) {
            let line = first_line + found.line - 1;
            match found.code {
                "FS-U016" => self.add(
                    "FS-U016",
                    file,
                    line,
                    format!(
                        "The script uses the browser's own animation interface ({}); it is not supported in this version.",
                        found.what
                    ),
                    "Remove the call and animate with a motion block instead.",
                ),
                _ => self.add(
                    "FS-U017",
                    file,
                    line,
                    "The script uses the framesmith script interface for building motion; it is not supported in this version.",
                    "Write the motion as a motion block in the page instead.",
                ),
            }
        }
    }

    fn element(&mut self, element: &Element) {
        let file = ENTRY_NAME;
        let top = Path::new("");
        let line = element.line;
        match element.tag.as_str() {
            "video" => self.add(
                "FS-U001",
                file,
                line,
                "The page has a <video> element; video is not supported in this version.",
                "Remove the element, or replace it with a still image.",
            ),
            "audio" => self.add(
                "FS-U002",
                file,
                line,
                "The page has an <audio> element; audio is not supported in this version.",
                "Remove the element. The rendered file has no sound in this version.",
            ),
            "iframe" | "embed" | "object" => self.add(
                "FS-U018",
                file,
                line,
                format!(
                    "The page embeds another document with <{}>; embedded documents are not supported in this version.",
                    element.tag
                ),
                "Remove the element and put its content in the page itself.",
            ),
            "animate" | "animatetransform" | "animatemotion" | "set" if element.foreign => self.add(
                "FS-U019",
                file,
                line,
                format!(
                    "The inline SVG has an animation element (<{}>); SVG animation is not supported in this version.",
                    element.tag
                ),
                "Remove the element and animate the shape with a motion block instead.",
            ),
            "img" => {
                if let Some(src) = element.attr("src") {
                    self.picture(file, src.line, top, &src.value, "image");
                }
                self.srcset(element);
            }
            "source" => self.srcset(element),
            "image" | "use" | "feimage" if element.foreign => {
                for name in ["href", "xlink:href"] {
                    if let Some(href) = element.attr(name) {
                        self.picture(file, href.line, top, &href.value, "image");
                    }
                }
            }
            "link" => {
                let rel = element.value("rel").unwrap_or("").to_ascii_lowercase();
                let rels: Vec<&str> = rel.split_ascii_whitespace().collect();
                let Some(href) = element.attr("href") else {
                    return;
                };
                if rels.contains(&"stylesheet") {
                    if let Some(path) = self.reference(file, href.line, top, &href.value, "style sheet") {
                        self.style_file(&path, 0);
                    }
                } else if rels
                    .iter()
                    .any(|rel| matches!(*rel, "icon" | "preload" | "modulepreload" | "apple-touch-icon" | "mask-icon"))
                {
                    self.reference(file, href.line, top, &href.value, "file");
                }
            }
            "style" => self.style(file, element.text_line, false, top, &element.text, 0),
            "script" if !is_motion_block(element) => {
                let page_script = is_page_script(element);
                match element.attr("src") {
                    Some(src) => {
                        let found = self.reference(file, src.line, top, &src.value, "script");
                        if let (Some(path), true) = (found, page_script) {
                            if let Ok(bytes) = std::fs::read(self.root.join(&path)) {
                                self.script(&display(&path), 1, &String::from_utf8_lossy(&bytes));
                            }
                        }
                    }
                    None if page_script => self.script(file, element.text_line, &element.text),
                    None => {}
                }
            }
            _ => {}
        }
        if let Some(style) = element.attr("style") {
            self.style(file, style.line, true, top, &style.value, 0);
        }
    }

    fn srcset(&mut self, element: &Element) {
        let Some(srcset) = element.attr("srcset") else {
            return;
        };
        for candidate in srcset.value.split(',') {
            if let Some(url) = candidate.split_ascii_whitespace().next() {
                self.picture(ENTRY_NAME, srcset.line, Path::new(""), url, "image");
            }
        }
    }

    /// A composition in another page at the top of the project.
    fn other_pages(&mut self) {
        let Ok(entries) = std::fs::read_dir(self.root) else {
            return;
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .filter(|entry| entry.path().is_file())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| {
                let lower = name.to_ascii_lowercase();
                (lower.ends_with(".html") || lower.ends_with(".htm")) && name != ENTRY_NAME
            })
            .collect();
        names.sort();
        for name in names {
            let Ok(bytes) = std::fs::read(self.root.join(&name)) else {
                continue;
            };
            let dom = html::parse(&String::from_utf8_lossy(&bytes));
            for attr in dom.elements.iter().filter_map(|e| e.attr("data-fs-composition")) {
                self.add(
                    "FS-U005",
                    &name,
                    attr.line,
                    format!(
                        "{name} holds a composition (\"{}\"); a project holds one composition, in {ENTRY_NAME}, in this version.",
                        attr.value
                    ),
                    "Move the page to its own project folder, or remove data-fs-composition from it.",
                );
            }
        }
    }
}

fn is_nonzero_time(value: &str) -> bool {
    value.split(',').any(|part| {
        let part = part.trim();
        let number = part.trim_end_matches(|c: char| c.is_ascii_alphabetic());
        match number.parse::<f64>() {
            Ok(n) => n != 0.0,
            // Not a plain time (a variable, a keyword): it may be one.
            Err(_) => !matches!(part, "" | "initial" | "unset" | "inherit" | "revert"),
        }
    })
}

/// Checks the project at `project`.
pub fn check(project: &Path) -> CheckReport {
    let shown = project.display().to_string();
    if !project.is_dir() {
        let description = if project.exists() {
            format!("{shown} is not a folder.")
        } else {
            format!("The project folder {shown} does not exist.")
        };
        return CheckReport {
            diagnostics: vec![Diagnostic::new(
                "FS-E001",
                &shown,
                0,
                description,
                "Give the path of a project folder. `framesmith init <folder>` creates one.",
            )],
            manifest: None,
        };
    }
    let entry = project.join(ENTRY_NAME);
    let source = match std::fs::read(&entry) {
        Ok(bytes) if entry.is_file() => String::from_utf8_lossy(&bytes).into_owned(),
        _ => {
            return CheckReport {
                diagnostics: vec![Diagnostic::new(
                    "FS-E002",
                    &shown,
                    0,
                    format!("The project has no {ENTRY_NAME}."),
                    format!("Add {ENTRY_NAME} to the project folder. `framesmith init <folder>` creates a starter project."),
                )],
                manifest: None,
            };
        }
    };

    let dom = html::parse(&source);
    let (raw, page_index) = raw_doc(&dom);
    let draft = Draft::read(&raw, ENTRY_NAME);
    let root = draft.composition_element().map(|raw_index| page_index[raw_index]);
    let resolved = draft.finish(&mut |target| selector::matches_in(&dom, root, target));

    let mut scan = Scan {
        root: project,
        diagnostics: resolved.diagnostics,
        sheets_seen: BTreeSet::new(),
    };
    for element in &dom.elements {
        scan.element(element);
    }
    scan.other_pages();

    // Motion writes these three properties, so an author's own value on a
    // targeted element would be lost.
    let targeted: BTreeSet<usize> = resolved.targets.iter().flatten().copied().collect();
    for &index in &targeted {
        let Some(style) = dom.elements[index].attr("style") else {
            continue;
        };
        for declaration in css::scan(&style.value).declarations {
            if matches!(declaration.property.as_str(), "translate" | "rotate" | "scale") {
                scan.add(
                    "FS-U021",
                    ENTRY_NAME,
                    style.line,
                    format!("An element that motion targets sets \"{}\" in its style attribute; motion writes that property, so the two cannot be combined in this version.", declaration.property),
                    format!("Remove \"{}\" from the style attribute, or use transform for the fixed part.", declaration.property),
                );
            }
        }
    }

    let mut diagnostics = scan.diagnostics;
    diagnostics.sort_by(|a, b| {
        let key = |d: &Diagnostic| (d.file != ENTRY_NAME, d.file.clone(), d.line);
        key(a).cmp(&key(b))
    });
    diagnostics.dedup();
    CheckReport {
        diagnostics,
        manifest: resolved.manifest,
    }
}
