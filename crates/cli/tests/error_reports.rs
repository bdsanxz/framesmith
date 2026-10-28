//! What `framesmith check` prints for every refusal and failure: the code, the kind, the file and
//! line where there is one, one sentence on what is wrong and one on what to change
//! (`docs/spec/error-catalogue.md`), with or without a terminal that asks for colour.

use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use project::{Code, Kind};
use testkit::sound_files::Pcm;

mod common;

/// A new folder named `name` for one test, under the tests' scratch folder.
fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("error-reports").join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// How the terminal of the run is set: the variables that ask for colour, or for none.
#[derive(Clone, Copy, Debug)]
enum Terminal {
    /// Asks for colour in every way it can.
    AsksForColour,
    /// Asks for none: `NO_COLOR` and a terminal that cannot show any.
    AsksForNone,
    /// None of the variables is set.
    Unset,
}

/// Runs `framesmith` with `args` from `cwd` under the terminal described.
fn framesmith<I, S>(cwd: &Path, terminal: Terminal, args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(env!("CARGO_BIN_EXE_framesmith"));
    command
        .args(args)
        .current_dir(cwd)
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("FORCE_COLOR")
        .env_remove("TERM");
    match terminal {
        Terminal::AsksForColour => {
            command
                .env("CLICOLOR_FORCE", "1")
                .env("FORCE_COLOR", "1")
                .env("TERM", "xterm-256color");
        }
        Terminal::AsksForNone => {
            command.env("NO_COLOR", "1").env("TERM", "dumb");
        }
        Terminal::Unset => {}
    }
    command.output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// A project folder whose `composition.xml` holds `bytes`, and an empty folder to run from.
fn project(name: &str, bytes: &[u8]) -> (PathBuf, PathBuf) {
    let base = folder(name);
    let project = base.join("demo");
    let cwd = base.join("cwd");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&cwd).unwrap();
    fs::write(project.join("composition.xml"), bytes).unwrap();
    (project, cwd)
}

/// A project folder `demo` whose `framesmith.yaml` holds `yaml` and whose `composition.xml` is a
/// valid root, and an empty folder to run from.
fn project_with_file(name: &str, yaml: &[u8]) -> (PathBuf, PathBuf) {
    let (project, cwd) = project(name, br#"<composition version="1" width="640" height="360" duration="2"/>"#);
    fs::write(project.join("framesmith.yaml"), yaml).unwrap();
    (project, cwd)
}

/// A project folder `demo` holding each of `files` (a path from the folder and its bytes), and an empty folder
/// to run from.
fn project_with_files(name: &str, files: &[(String, Vec<u8>)]) -> (PathBuf, PathBuf) {
    let (project, cwd) = project(name, br#"<composition version="1" width="640" height="360" duration="2"/>"#);
    for (path, bytes) in files {
        let target = project.join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, bytes).unwrap();
    }
    (project, cwd)
}

/// A `<composition src>` on its own line, showing `src`.
fn shows(src: &str) -> String {
    format!(r#"<composition src="{src}" left="0" top="0"/>"#)
}

/// A composition that shows a file of a chain of `last` files: each of `f2.xml` to `f{last-1}.xml` shows the next,
/// and `f{last}.xml` is a valid root with a box. The root shows `f2.xml`.
fn chain_files(last: u32) -> Vec<(String, Vec<u8>)> {
    let mut files = vec![("composition.xml".to_string(), inside_root(&shows("f2.xml")))];
    for number in 2..last {
        files.push((format!("f{number}.xml"), inside_root(&shows(&format!("f{}.xml", number + 1)))));
    }
    files.push((
        format!("f{last}.xml"),
        inside_root(r#"<box left="0" top="0" width="4" height="4"/>"#),
    ));
    files
}

/// One project for `shown_problems`: its name, its files, and the code `check` must print.
type ShownProblem = (&'static str, Vec<(String, Vec<u8>)>, Code);

/// Projects whose composition shows another file and gives a problem of that. Each is run as a project of its own.
fn shown_problems() -> Vec<ShownProblem> {
    let at_30 = |src: &str| inside_root(&shows(src));
    let twenty_four = br#"<composition version="1" width="640" height="360" fps="24" duration="2"/>"#.to_vec();
    vec![
        (
            "shown-loop",
            vec![
                ("composition.xml".to_string(), at_30("b.xml")),
                ("b.xml".to_string(), at_30("composition.xml")),
            ],
            Code::ShownLoop,
        ),
        ("shown-too-deep", chain_files(17), Code::ShownTooDeep),
        (
            "border-not-in-form",
            vec![("composition.xml".to_string(), inside_root(r##"<box border="2px #ff0000"/>"##))],
            Code::BorderNotInForm,
        ),
        (
            "shadow-not-in-form",
            vec![("composition.xml".to_string(), inside_root(r#"<box box-shadow="0 0 4 red"/>"#))],
            Code::ShadowNotInForm,
        ),
        (
            "tween-no-duration",
            vec![("composition.xml".to_string(), inside_root(r#"<box><tween to-x="5"/></box>"#))],
            Code::TweenNoDuration,
        ),
        (
            "tween-no-property",
            vec![("composition.xml".to_string(), inside_root(r#"<box><tween duration="1"/></box>"#))],
            Code::TweenNoProperty,
        ),
        (
            "property-not-taken",
            vec![(
                "composition.xml".to_string(),
                inside_root(r#"<box><tween duration="1" to-count="5"/></box>"#),
            )],
            Code::PropertyNotTaken,
        ),
        (
            "set-no-property",
            vec![("composition.xml".to_string(), inside_root(r#"<box><set at="1"/></box>"#))],
            Code::SetNoProperty,
        ),
        (
            "set-property-not-taken",
            vec![("composition.xml".to_string(), inside_root(r#"<box><set at="1" count="5"/></box>"#))],
            Code::SetPropertyNotTaken,
        ),
        (
            "ease-not-known",
            vec![(
                "composition.xml".to_string(),
                inside_root(r#"<box><tween duration="1" to-x="5" ease="bounce"/></box>"#),
            )],
            Code::EaseNotKnown,
        ),
        (
            "background-not-in-form",
            vec![(
                "composition.xml".to_string(),
                inside_root(r##"<box background="linear-gradient(90deg, #ff0000 0%)"/>"##),
            )],
            Code::BackgroundNotInForm,
        ),
        (
            "background-list-length",
            vec![(
                "composition.xml".to_string(),
                inside_root(r##"<box background="#ff0000, #00ff00, #0000ff" background-size="10 10, 20 20"/>"##),
            )],
            Code::BackgroundListLength,
        ),
        (
            "background-setting-alone",
            vec![("composition.xml".to_string(), inside_root(r#"<box background-size="10 10"/>"#))],
            Code::BackgroundSettingAlone,
        ),
        (
            "shown-frame-rate",
            vec![
                ("composition.xml".to_string(), at_30("inner.xml")),
                ("inner.xml".to_string(), twenty_four),
            ],
            Code::ShownFpsDiffers,
        ),
        (
            "shown-missing",
            vec![("composition.xml".to_string(), at_30("gone.xml"))],
            Code::ShownMissing,
        ),
        (
            // A path that names a folder cannot be read as a file, on every system.
            "shown-unreadable",
            vec![
                ("composition.xml".to_string(), at_30("scenes")),
                (
                    "scenes/keep.xml".to_string(),
                    inside_root(r#"<box left="0" top="0" width="4" height="4"/>"#),
                ),
            ],
            Code::ShownUnreadable,
        ),
    ]
}

/// Projects whose `<font>` names a file that is missing, that cannot be read, or that is not a font. Each is run as a
/// project of its own, like `shown_problems`.
fn font_problems() -> Vec<ShownProblem> {
    let font = |src: &str| inside_root(&format!("<font src=\"{src}\" family=\"Test\"/>"));
    vec![
        (
            "font-missing",
            vec![("composition.xml".to_string(), font("fonts/gone.ttf"))],
            Code::FontMissing,
        ),
        (
            // A path that names a folder names no file, on every system.
            "font-names-a-folder",
            vec![
                ("composition.xml".to_string(), font("scenes")),
                (
                    "scenes/keep.xml".to_string(),
                    inside_root(r#"<box left="0" top="0" width="4" height="4"/>"#),
                ),
            ],
            Code::FontMissing,
        ),
        (
            "font-not-a-font",
            vec![
                ("composition.xml".to_string(), font("fonts/notes.ttf")),
                ("fonts/notes.ttf".to_string(), b"These bytes are not a font.\n".to_vec()),
            ],
            Code::FontNotAFont,
        ),
        (
            // The TrueType tag, and nothing after it that the font reader can read.
            "font-damaged",
            vec![
                ("composition.xml".to_string(), font("fonts/damaged.ttf")),
                ("fonts/damaged.ttf".to_string(), b"\x00\x01\x00\x00 and no table directory".to_vec()),
            ],
            Code::FontDamaged,
        ),
    ]
}

/// The text of `docs/spec/error-catalogue.md`.
fn catalogue() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("docs/spec/error-catalogue.md");
    fs::read_to_string(path).unwrap()
}

/// Projects of one `<audio>` that `check` refuses as invalid (`docs/spec/sound.md`, section 5).
fn sound_problems() -> Vec<ShownProblem> {
    let audio = |attributes: &str| inside_root(&format!("<audio {attributes}/>"));
    let good = testkit::sound_files::wav_bytes(48_000, 2, Pcm::Int(16), &[0.0; 2_000]);
    // The same file, cut short: its header still says it holds 4,000 bytes of sound, and 2,000 are there.
    let cut = {
        let mut bytes = good.clone();
        bytes.truncate(good.len() - 2_000);
        bytes
    };
    vec![
        (
            "sound-no-duration",
            vec![
                ("composition.xml".to_string(), audio(r#"src="tone.wav""#)),
                ("tone.wav".to_string(), good.clone()),
            ],
            Code::SoundNoDuration,
        ),
        (
            "sound-missing",
            vec![("composition.xml".to_string(), audio(r#"src="gone.wav" duration="1""#))],
            Code::SoundMissing,
        ),
        (
            "sound-damaged",
            vec![
                ("composition.xml".to_string(), audio(r#"src="cut.wav" duration="1""#)),
                ("cut.wav".to_string(), cut),
            ],
            Code::SoundDamaged,
        ),
        (
            "sound-start-past-end",
            vec![
                (
                    "composition.xml".to_string(),
                    audio(r#"src="tone.wav" duration="1" media-start="1""#),
                ),
                ("tone.wav".to_string(), good),
            ],
            Code::SoundStartPastEnd,
        ),
    ]
}

/// A valid root with `body` inside it, starting on line 2.
fn inside_root(body: &str) -> Vec<u8> {
    format!("<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\">\n{body}\n</composition>\n").into_bytes()
}

/// Composition files that each have one wrong value (of the root or of what is in it): the name, the bytes, the code the
/// command must report and the line it must report it on.
fn wrong_roots() -> Vec<(&'static str, Vec<u8>, Code, u32)> {
    let mut not_utf8 = b"<composition version=\"1\"\n id=\"".to_vec();
    not_utf8.push(0xff);
    not_utf8.extend_from_slice(b"\"/>");
    let good = "width=\"640\" height=\"360\" duration=\"2\"";
    let root = |attributes: &str| format!("<?xml version=\"1.0\"?>\n<composition\n {attributes}/>").into_bytes();
    vec![
        ("not-utf-8", not_utf8, Code::NotUtf8, 2),
        ("not-xml", b"<composition version=\"1\">\n</box>".to_vec(), Code::NotXml, 2),
        ("not-a-composition", b"\n<html/>".to_vec(), Code::RootNotComposition, 2),
        ("no-version", root(good), Code::VersionMissing, 2),
        ("version-2", root(&format!("version=\"2\" {good}")), Code::VersionNotRead, 3),
        (
            "no-width",
            root("version=\"1\" height=\"360\" duration=\"2\""),
            Code::SizeMissing,
            2,
        ),
        (
            "odd-width",
            root("version=\"1\" width=\"641\" height=\"360\" duration=\"2\""),
            Code::SizeNotAccepted,
            3,
        ),
        (
            "fps-zero",
            root(&format!("version=\"1\" fps=\"0\" {good}")),
            Code::FpsNotAccepted,
            3,
        ),
        (
            "no-duration",
            root("version=\"1\" width=\"640\" height=\"360\""),
            Code::DurationMissing,
            2,
        ),
        (
            "duration-negative",
            root("version=\"1\" width=\"640\" height=\"360\" duration=\"-5\""),
            Code::DurationNotATime,
            3,
        ),
        (
            "duration-zero",
            root("version=\"1\" width=\"640\" height=\"360\" duration=\"0\""),
            Code::DurationNotAboveZero,
            3,
        ),
        (
            "background-name",
            root(&format!("version=\"1\" background=\"red\" {good}")),
            Code::BackgroundNotAColour,
            3,
        ),
        ("unknown-element", inside_root("<circle/>"), Code::ElementNotKnown, 2),
        (
            "attribute-not-taken",
            inside_root("<rect colour=\"red\"/>"),
            Code::AttributeNotTaken,
            2,
        ),
        (
            "element-in-the-wrong-place",
            inside_root("<text>\n<box/>\n</text>"),
            Code::ElementNotHere,
            3,
        ),
        ("x-is-not-a-number", inside_root("<box x=\"12px\"/>"), Code::NotANumber, 2),
        (
            "at-is-not-a-time",
            inside_root("<box>\n<set at=\"5.\" x=\"1\"/>\n</box>"),
            Code::NotATime,
            3,
        ),
        ("start-without-duration", inside_root("<box start=\"1\"/>"), Code::ClipNotPaired, 2),
        ("style-without-id", inside_root("<style/>"), Code::StyleWithoutId, 2),
        (
            "style-id-twice",
            inside_root("<style id=\"a\"/>\n<style id=\"a\"/>"),
            Code::StyleIdTwice,
            3,
        ),
        (
            "class-names-no-style",
            inside_root("<box class=\"a\"/>"),
            Code::ClassNamesNoStyle,
            2,
        ),
        (
            "letter-spacing-not-taken",
            inside_root("<box letter-spacing=\"x\" width=\"4\" height=\"4\"/>"),
            Code::ValueNotInForm,
            2,
        ),
        (
            "font-size-zero",
            inside_root("<box font-size=\"0\" width=\"4\" height=\"4\"/>"),
            Code::SizeNotAboveZero,
            2,
        ),
        (
            "columns-not-a-list",
            inside_root("<box columns=\"wide 1fr\" width=\"4\" height=\"4\"/>"),
            Code::ColumnsNotAList,
            2,
        ),
        (
            "columns-with-direction",
            inside_root("<box columns=\"640 1fr\" flex-direction=\"row\" width=\"4\" height=\"4\"/>"),
            Code::ColumnsWithDirection,
            2,
        ),
    ]
}

/// Project files that each have one wrong key: the name, the bytes, the code the command must
/// report for the file and the line it must report it on.
fn wrong_project_files() -> Vec<(&'static str, Vec<u8>, Code, u32)> {
    vec![
        ("project-not-utf-8", b"name: demo\n\xff\n".to_vec(), Code::ProjectFileNotUtf8, 2),
        (
            "project-not-yaml",
            b"name: demo\nname: other\n".to_vec(),
            Code::ProjectFileNotYaml,
            2,
        ),
        (
            "project-not-a-mapping",
            b"- name\n- demo\n".to_vec(),
            Code::ProjectFileNotAMapping,
            1,
        ),
        ("project-unknown-key", b"name: demo\nspeed: fast\n".to_vec(), Code::KeyNotListed, 2),
        (
            "project-unsupported-key",
            b"name: demo\ntimeline: []\n".to_vec(),
            Code::KeyNotSupported,
            2,
        ),
        (
            "project-wrong-value",
            b"render:\n  codec: vp9\n".to_vec(),
            Code::ValueNotAccepted,
            2,
        ),
    ]
}

/// The parts of one report line: `file, line N: code: kind: sentence. remedy`.
struct Parts {
    file: String,
    line: u32,
    code: String,
    kind: String,
    sentence: String,
    remedy: String,
}

/// Splits a report line into its parts, knowing the remedy of the code it must carry.
fn split(report: &str, remedy: &str) -> Parts {
    let (place, rest) = report.split_once(": ").unwrap_or_else(|| panic!("no place: {report}"));
    let (file, line) = place.split_once(", line ").unwrap_or_else(|| panic!("no file and line: {report}"));
    let (code, rest) = rest.split_once(": ").unwrap_or_else(|| panic!("no code: {report}"));
    let (kind, rest) = rest.split_once(": ").unwrap_or_else(|| panic!("no kind: {report}"));
    let sentence = rest
        .strip_suffix(&format!(" {remedy}"))
        .unwrap_or_else(|| panic!("the line does not end with the remedy {remedy:?}: {report}"));
    Parts {
        file: file.to_string(),
        line: line.parse().unwrap_or_else(|_| panic!("no line number: {report}")),
        code: code.to_string(),
        kind: kind.to_string(),
        sentence: sentence.to_string(),
        remedy: remedy.to_string(),
    }
}

#[test]
fn every_problem_of_a_wrong_root_shows_code_kind_file_line_what_is_wrong_and_what_to_change() {
    let catalogue = catalogue();
    for (name, bytes, code, line) in wrong_roots() {
        let (project, cwd) = project(name, &bytes);
        let output = framesmith(&cwd, Terminal::AsksForNone, [OsStr::new("check"), project.as_os_str()]);
        assert_eq!(output.status.code(), Some(1), "{name}");
        let printed = text(&output.stdout);
        let reports: Vec<&str> = printed.lines().filter(|line| !line.starts_with("Invalid:")).collect();
        assert_eq!(reports.len(), 1, "{name}: {printed}");

        let parts = split(reports[0], code.remedy());
        assert_eq!(parts.file, "composition.xml", "{name}");
        assert_eq!(parts.line, line, "{name}");
        assert_eq!(parts.code, code.as_str(), "{name}");
        assert_eq!(parts.kind, "invalid", "{name}");
        // What is wrong: one sentence, with its full stop and no second one.
        assert!(
            parts.sentence.ends_with('.') && !parts.sentence.contains(". "),
            "{name}: {}",
            parts.sentence
        );
        assert!(parts.sentence.len() > 10, "{name}: {}", parts.sentence);
        // What to change: the catalogue's own sentence, word for word.
        assert!(catalogue.contains(&format!("| {} |", parts.remedy)), "{name}: {}", parts.remedy);
        assert!(
            catalogue.contains(&format!("| `{}` | invalid |", parts.code)),
            "{name}: {} is not listed as invalid",
            parts.code
        );
    }
}

#[test]
fn every_problem_of_a_wrong_project_file_shows_code_kind_file_line_what_is_wrong_and_what_to_change() {
    let catalogue = catalogue();
    for (name, bytes, code, line) in wrong_project_files() {
        let (project, cwd) = project_with_file(name, &bytes);
        let output = framesmith(&cwd, Terminal::AsksForNone, [OsStr::new("check"), project.as_os_str()]);
        // Invalid ends with 1; a project that is right except for what is not supported yet, with 2.
        let (exit, closing) = match code.kind() {
            Kind::Unsupported => (2, "Unsupported:"),
            _ => (1, "Invalid:"),
        };
        assert_eq!(output.status.code(), Some(exit), "{name}");
        assert_eq!(text(&output.stderr), "", "{name}");
        let printed = text(&output.stdout);
        let reports: Vec<&str> = printed.lines().filter(|line| !line.starts_with(closing)).collect();
        assert_eq!(reports.len(), 1, "{name}: {printed}");
        assert_eq!(printed.lines().count(), 2, "{name}: {printed}");

        let parts = split(reports[0], code.remedy());
        assert_eq!(parts.file, "framesmith.yaml", "{name}");
        assert_eq!(parts.line, line, "{name}");
        assert_eq!(parts.code, code.as_str(), "{name}");
        assert_eq!(parts.kind, code.kind().as_str(), "{name}");
        assert!(
            parts.sentence.ends_with('.') && !parts.sentence.contains(". "),
            "{name}: {}",
            parts.sentence
        );
        assert!(parts.sentence.len() > 10, "{name}: {}", parts.sentence);
        assert!(catalogue.contains(&format!("| {} |", parts.remedy)), "{name}: {}", parts.remedy);
        assert!(
            catalogue.contains(&format!("| `{}` | {} |", parts.code, code.kind().as_str())),
            "{name}: {} is not listed as {}",
            parts.code,
            code.kind().as_str()
        );
    }
}

#[test]
fn a_report_without_colour_still_shows_the_code_the_kind_and_the_remedy() {
    for terminal in [Terminal::AsksForNone, Terminal::AsksForColour, Terminal::Unset] {
        for (name, bytes, code, _) in wrong_roots() {
            let (project, cwd) = project(&format!("{name}-{terminal:?}"), &bytes);
            let output = framesmith(&cwd, terminal, [OsStr::new("check"), project.as_os_str()]);
            for stream in [&output.stdout, &output.stderr] {
                assert!(!stream.contains(&0x1b), "{name} {terminal:?}: an escape byte");
                assert!(
                    stream.iter().all(|&byte| byte == b'\n' || (0x20..0x7f).contains(&byte)),
                    "{name} {terminal:?}: {stream:?}"
                );
            }
            let printed = text(&output.stdout);
            let report = printed.lines().next().unwrap();
            assert!(report.contains(&format!(": {code}: invalid: ")), "{name} {terminal:?}: {report}");
            assert!(report.ends_with(code.remedy()), "{name} {terminal:?}: {report}");
        }
        for (name, bytes, code, _) in wrong_project_files() {
            let (project, cwd) = project_with_file(&format!("{name}-{terminal:?}"), &bytes);
            let output = framesmith(&cwd, terminal, [OsStr::new("check"), project.as_os_str()]);
            for stream in [&output.stdout, &output.stderr] {
                assert!(!stream.contains(&0x1b), "{name} {terminal:?}: an escape byte");
                assert!(
                    stream.iter().all(|&byte| byte == b'\n' || (0x20..0x7f).contains(&byte)),
                    "{name} {terminal:?}: {stream:?}"
                );
            }
            let printed = text(&output.stdout);
            let report = printed.lines().next().unwrap();
            assert!(
                report.contains(&format!(": {code}: {}: ", code.kind().as_str())),
                "{name} {terminal:?}: {report}"
            );
            assert!(report.ends_with(code.remedy()), "{name} {terminal:?}: {report}");
        }
    }
}

/// A run that ends in `No project`, or in a command line that cannot be read: what it printed on
/// standard output and on standard error, and the code it must carry.
fn refusals() -> Vec<(&'static str, String, String, Code)> {
    let base = folder("refusals");
    let cwd = base.join("cwd");
    fs::create_dir_all(&cwd).unwrap();
    fs::create_dir_all(base.join("empty")).unwrap();
    fs::write(base.join("a-file"), "<composition/>").unwrap();
    fs::create_dir_all(base.join("unreadable").join("composition.xml")).unwrap();
    // A composition file given on the command line that this user may not read.
    fs::write(base.join("no-access.xml"), "<composition/>").unwrap();
    fs::set_permissions(base.join("no-access.xml"), fs::Permissions::from_mode(0o000)).unwrap();
    let valid_root = r#"<composition version="1" width="640" height="360" duration="2"/>"#;
    // A project file that is a folder, a composition named and not there, and one named that is a folder.
    fs::create_dir_all(base.join("project-file-unreadable").join("framesmith.yaml")).unwrap();
    fs::write(base.join("project-file-unreadable").join("composition.xml"), valid_root).unwrap();
    fs::create_dir_all(base.join("named-missing")).unwrap();
    fs::write(base.join("named-missing").join("framesmith.yaml"), "composition: nope.xml\n").unwrap();
    fs::create_dir_all(base.join("named-unreadable").join("scenes")).unwrap();
    fs::write(base.join("named-unreadable").join("framesmith.yaml"), "composition: scenes\n").unwrap();
    let run = |args: Vec<&str>| {
        let shown = format!("{args:?}");
        let output = framesmith(&cwd, Terminal::AsksForNone, args);
        assert_eq!(output.status.code(), Some(1), "{shown}");
        (text(&output.stdout), text(&output.stderr))
    };
    let mut runs = Vec::new();
    for (name, folder, code) in [
        ("folder-missing", "../nothing-here/", Code::FolderMissing),
        ("not-a-folder", "../a-file/", Code::NotAFolder),
        ("path-missing", "../nothing-here", Code::PathMissing),
        ("no-composition", "../empty", Code::NoComposition),
        ("composition-unreadable", "../unreadable", Code::CompositionUnreadable),
        ("path-unreadable", "../no-access.xml", Code::PathUnreadable),
        ("project-file-unreadable", "../project-file-unreadable", Code::ProjectFileUnreadable),
        ("named-composition-missing", "../named-missing", Code::NamedCompositionMissing),
        (
            "named-composition-unreadable",
            "../named-unreadable",
            Code::NamedCompositionUnreadable,
        ),
    ] {
        let (out, err) = run(vec!["check", folder]);
        runs.push((name, out, err, code));
    }
    let (out, err) = run(vec!["check"]);
    runs.push(("command-line", out, err, Code::CommandLineNotRead));
    let (out, err) = run(vec!["render", "../empty", "--quality", "ultra"]);
    runs.push(("option-not-taken", out, err, Code::OptionNotTaken));
    // Give the file back its owner's permissions, so that the folder can be removed.
    fs::set_permissions(base.join("no-access.xml"), fs::Permissions::from_mode(0o644)).unwrap();
    runs
}

/// Compositions and project files that `render` refuses or cannot carry out, with the code each
/// must carry and the exit code: a length that is wrong, a form that is not drawn, a film that
/// cannot be made (the place of the file is inside a file).
fn render_refusals() -> Vec<(&'static str, String, Option<&'static str>, Code, i32)> {
    let root = |inside: &str| format!(r#"<composition version="1" width="640" height="360" duration="2">{inside}</composition>"#);
    vec![
        ("length-wrong", root(r#"<box width="abc"/>"#), None, Code::LengthNotAccepted, 1),
        (
            "position-not-a-word",
            root(r#"<box position="fixed"/>"#),
            None,
            Code::WordNotTaken,
            1,
        ),
        ("form-not-drawn", root("<rect/>"), None, Code::FormNotSupported, 2),
        (
            "output-not-made",
            root(r##"<box background="#ff0000"/>"##),
            Some("render:\n  output: composition.xml/film.mp4\n"),
            Code::OutputNotWritten,
            2,
        ),
    ]
}

#[test]
fn a_refusal_with_no_file_to_name_shows_its_code_its_kind_and_its_remedy() {
    let catalogue = catalogue();
    for (name, out, err, code) in refusals() {
        let report = if matches!(code, Code::CommandLineNotRead | Code::OptionNotTaken) {
            assert_eq!(out, "", "{name}");
            err.lines().next().unwrap().strip_prefix("framesmith: ").unwrap().to_string()
        } else {
            assert_eq!(err, "", "{name}");
            assert_eq!(out.lines().count(), 1, "{name}: {out}");
            out.lines().next().unwrap().strip_prefix("No project: ").unwrap().to_string()
        };
        assert!(
            report.starts_with(&format!("{code}: {}: ", code.kind().as_str())),
            "{name}: {report}"
        );
        assert!(report.ends_with(&format!(". {}", code.remedy())), "{name}: {report}");
        assert!(catalogue.contains(&format!("| {} |", code.remedy())), "{name}");
    }
    // The machine's refusal to read is a failure; a path that is wrong is invalid.
    assert_eq!(Code::CompositionUnreadable.kind(), Kind::Failure);
    assert_eq!(Code::FolderMissing.kind(), Kind::Invalid);
}

#[test]
fn every_code_of_the_list_is_emitted_by_the_command_on_some_input() {
    let mut emitted = BTreeSet::new();
    for (name, bytes, code, _) in wrong_roots() {
        let (project, cwd) = project(&format!("emit-{name}"), &bytes);
        let output = framesmith(&cwd, Terminal::AsksForNone, [OsStr::new("check"), project.as_os_str()]);
        let printed = text(&output.stdout);
        assert!(printed.contains(&format!(": {code}: ")), "{name}: {printed}");
        emitted.insert(code.as_str());
    }
    for (name, bytes, code, _) in wrong_project_files() {
        let (project, cwd) = project_with_file(&format!("emit-{name}"), &bytes);
        let output = framesmith(&cwd, Terminal::AsksForNone, [OsStr::new("check"), project.as_os_str()]);
        let printed = text(&output.stdout);
        assert!(printed.contains(&format!(": {code}: ")), "{name}: {printed}");
        emitted.insert(code.as_str());
    }
    for (name, out, err, code) in refusals() {
        assert!(out.contains(code.as_str()) || err.contains(code.as_str()), "{name}");
        emitted.insert(code.as_str());
    }
    for (name, composition, yaml, code, exit) in render_refusals() {
        let (project, cwd) = project(&format!("emit-{name}"), composition.as_bytes());
        if let Some(yaml) = yaml {
            fs::write(project.join("framesmith.yaml"), yaml).unwrap();
        }
        let output = framesmith(&cwd, Terminal::AsksForNone, [OsStr::new("render"), project.as_os_str()]);
        let printed = text(&output.stdout);
        assert_eq!(output.status.code(), Some(exit), "{name}: {printed}");
        // A failure has no place in a file: its report begins with the code.
        assert!(printed.contains(&format!("{code}: ")), "{name}: {printed}");
        emitted.insert(code.as_str());
    }
    for (name, files, code) in shown_problems().into_iter().chain(font_problems()).chain(sound_problems()) {
        let (project, cwd) = project_with_files(&format!("emit-{name}"), &files);
        let output = framesmith(&cwd, Terminal::AsksForNone, [OsStr::new("check"), project.as_os_str()]);
        let printed = text(&output.stdout);
        assert_eq!(output.status.code(), Some(1), "{name}: {printed}");
        assert!(printed.contains(&format!(": {code}: ")), "{name}: {printed}");
        emitted.insert(code.as_str());
    }
    // A font file that is there and cannot be read: its permissions take every read away, so the file is found and
    // its read fails. Such a test runs as a user who is not root, as every hosted runner does.
    let (locked, locked_cwd) = project_with_files(
        "emit-font-unreadable",
        &[
            (
                "composition.xml".to_string(),
                inside_root(r#"<font src="fonts/locked.ttf" family="Test"/>"#),
            ),
            ("fonts/locked.ttf".to_string(), b"no one reads this font\n".to_vec()),
        ],
    );
    fs::set_permissions(locked.join("fonts").join("locked.ttf"), fs::Permissions::from_mode(0o000)).unwrap();
    let output = framesmith(&locked_cwd, Terminal::AsksForNone, [OsStr::new("check"), locked.as_os_str()]);
    let printed = text(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "{printed}");
    assert!(printed.contains(&format!(": {}: ", Code::FontUnreadable.as_str())), "{printed}");
    emitted.insert(Code::FontUnreadable.as_str());

    // A sound file that is there and cannot be read, in the same way; and one of a kind not read, which is unsupported.
    let (locked, locked_cwd) = project_with_files(
        "emit-sound-unreadable",
        &[
            (
                "composition.xml".to_string(),
                inside_root(r#"<audio src="locked.wav" duration="1"/>"#),
            ),
            (
                "locked.wav".to_string(),
                testkit::sound_files::wav_bytes(48_000, 2, Pcm::Int(16), &[0.0; 2_000]),
            ),
        ],
    );
    fs::set_permissions(locked.join("locked.wav"), fs::Permissions::from_mode(0o000)).unwrap();
    let output = framesmith(&locked_cwd, Terminal::AsksForNone, [OsStr::new("check"), locked.as_os_str()]);
    let printed = text(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "{printed}");
    assert!(printed.contains(&format!(": {}: ", Code::SoundUnreadable.as_str())), "{printed}");
    emitted.insert(Code::SoundUnreadable.as_str());
    let (ulaw, ulaw_cwd) = project_with_files(
        "emit-sound-not-read",
        &[
            (
                "composition.xml".to_string(),
                inside_root(r#"<audio src="ulaw.wav" duration="1"/>"#),
            ),
            (
                "ulaw.wav".to_string(),
                testkit::sound_files::wav_bytes(48_000, 1, Pcm::MuLaw, &[0.0; 4_000]),
            ),
        ],
    );
    let output = framesmith(&ulaw_cwd, Terminal::AsksForNone, [OsStr::new("check"), ulaw.as_os_str()]);
    let printed = text(&output.stdout);
    assert_eq!(output.status.code(), Some(2), "{printed}");
    assert!(printed.contains(&format!(": {}: ", Code::SoundNotRead.as_str())), "{printed}");
    emitted.insert(Code::SoundNotRead.as_str());

    // The card's refusal is what a machine with no graphics card gives, and such a machine is not
    // needed to see it: the command runs in this process, through the same entry the binary runs, with
    // the starter of a card that does not start (`cli::run_with`). Every other run here names its
    // folder for its own child process, so the working folder of this process is free to change.
    let (gpu_project, gpu_cwd) = project(
        "emit-gpu-without-a-card",
        r#"<composition version="1" width="640" height="360" duration="2"/>"#.as_bytes(),
    );
    std::env::set_current_dir(&gpu_cwd).unwrap();
    let args = vec![
        OsString::from("render"),
        gpu_project.as_os_str().to_owned(),
        OsString::from("--backend"),
        OsString::from("gpu"),
    ];
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let code = cli::run_with(&args, &mut out, &mut err, &card_does_not_start);
    let printed = text(&out);
    assert_eq!(code, 2, "{printed}");
    assert!(
        printed.starts_with(&format!("{}: ", Code::RendererNotAvailable.as_str())),
        "{printed}"
    );
    emitted.insert(Code::RendererNotAvailable.as_str());

    // The one refusal of an emoji with no picture is emitted only where the Mac's emoji font is read: that font is a file of
    // the machine, so no other system reads it, and no emoji is drawn there (`docs/spec/text.md`, sections 2.6 and 12.4).
    #[cfg(target_os = "macos")]
    {
        let composition = r##"<composition version="1" width="320" height="300" fps="10" duration="1" background="#ffffff">
<font src="/System/Library/Fonts/Apple Color Emoji.ttc" family="Apple Color Emoji"/>
<text left="10" top="10" width="300" font-family="Apple Color Emoji" font-size="20" line-height="2" color="#1e6fd9">♀</text>
</composition>
"##;
        let (project, cwd) = project("emit-emoji-without-picture", composition.as_bytes());
        let output = framesmith(&cwd, Terminal::AsksForNone, [OsStr::new("check"), project.as_os_str()]);
        let printed = text(&output.stdout);
        assert!(printed.contains(&format!(": {}: ", Code::EmojiNotDrawn.as_str())), "{printed}");
        emitted.insert(Code::EmojiNotDrawn.as_str());
    }

    // A film whose write stops after its first frames: no input of the list above reaches it, so the
    // file-size limit of the shell stops the write, which the command meets as an ordinary write failure.
    let composition = r##"<composition version="1" width="320" height="120" fps="30" duration="4" background="#ffffff"><box left="0" top="40" width="40" height="40" background="#ff0000"><tween duration="4" to-x="260"/></box></composition>"##;
    let (project, cwd) = project("emit-film-write-stopped", composition.as_bytes());
    let output = cwd.join("film.mp4");
    let rendered = common::framesmith_with_file_limit(&cwd, ["render", project.to_str().unwrap(), "--output", output.to_str().unwrap()]);
    let printed = text(&rendered.stdout);
    assert_eq!(rendered.status.code(), Some(1), "{printed}");
    assert!(printed.contains("FS-0036: failure: the video could not be made: "), "{printed}");
    emitted.insert(Code::VideoNotMade.as_str());

    let mut all: BTreeSet<&str> = Code::ALL.iter().map(|code| code.as_str()).collect();
    if !cfg!(target_os = "macos") {
        all.remove(Code::EmojiNotDrawn.as_str());
    }
    assert_eq!(emitted, all);
}

/// The starter of a machine with no graphics card: the card cannot be started, and the reason says so.
fn card_does_not_start() -> Result<Box<dyn draw::Renderer>, draw::Error> {
    Err(draw::Error::new(
        "this machine has no graphics card frx7 can draw with: no adapter was found",
    ))
}
