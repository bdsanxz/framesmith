//! Every entry of the support table (`docs/spec/notation.md` section 14) has a fixture under `fixtures/support-table`, and
//! for each, `check` lists the entry on the fixture's file and line with the entry's reason and sprint, and exits 2;
//! `render` prints the same list, exits 2, draws no frame, leaves no part file and keeps the bytes of a film already at its
//! output path. A project with one unsupported and one invalid form lists both in one run and exits 1; a form no entry names
//! is invalid (`docs/spec/check-command.md` section 3d). The expected file and line of each fixture are written beside it,
//! worked from the fixture by hand; the reason and the sprint are read from the spec, not from the program.

mod common;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use common::{err, framesmith, out, scratch};

/// The mark of an entry that is told by reading a media file, which T089 brings (section 14).
const MEDIA_FILE_MARK: &str = "told by reading the media file (T089)";

/// An entry of section 14: its name (`S36`), how it is recognised, its reason and when it is brought, as written.
struct Row {
    name: String,
    recognised: String,
    reason: String,
    waits: String,
}

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rows() -> Vec<Row> {
    let text = fs::read_to_string(repository().join("docs/spec/notation.md")).unwrap();
    let section = text
        .split("## 14. The support table")
        .nth(1)
        .and_then(|rest| rest.split("\n## 15.").next())
        .expect("the spec has a section 14");
    section
        .lines()
        .filter_map(|line| {
            let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
            let number: u32 = cells.first()?.strip_prefix('S')?.parse().ok()?;
            Some(Row {
                name: format!("S{number:02}"),
                recognised: cells[2].to_string(),
                reason: cells[3].to_string(),
                waits: cells[4].to_string(),
            })
        })
        .collect()
}

/// The entries that have a fixture: all but those marked as told by a media file, whose names are
/// printed so that a run shows what it leaves out.
fn with_fixtures() -> Vec<Row> {
    let (left_out, kept): (Vec<Row>, Vec<Row>) = rows()
        .into_iter()
        .partition(|row| row.recognised.contains(MEDIA_FILE_MARK));
    let names: Vec<&str> = left_out.iter().map(|row| row.name.as_str()).collect();
    println!("entries without a fixture, by their mark: {}", names.join(", "));
    kept
}

fn fixture(row: &Row) -> PathBuf {
    repository().join("fixtures/support-table").join(row.name.to_lowercase())
}

/// The file and the line the fixture's report is expected on, as written in its `expected`.
fn expected(row: &Row) -> (String, u32) {
    let text = fs::read_to_string(fixture(row).join("expected")).unwrap();
    let (file, line) = text.trim().split_once(' ').unwrap();
    (file.to_string(), line.parse().unwrap())
}

/// Copies a folder and what it holds.
fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// A copy of a fixture to run, with the repository's test fonts in its `fonts` folder, and the folder to run from.
fn project_of(group: &str, name: &str, from: &Path) -> (PathBuf, PathBuf) {
    let base = scratch(group, name);
    let project = base.join("demo");
    let cwd = base.join("cwd");
    copy(from, &project);
    copy(&repository().join("fixtures/fonts"), &project.join("fonts"));
    fs::remove_file(project.join("expected")).ok();
    fs::create_dir_all(&cwd).unwrap();
    (project, cwd)
}

/// Every file under a folder, as paths from it.
fn files_under(folder: &Path) -> BTreeSet<PathBuf> {
    let mut found = BTreeSet::new();
    let mut pending = vec![folder.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                found.insert(path.strip_prefix(folder).unwrap().to_path_buf());
            }
        }
    }
    found
}

/// The film's path in a fixture project: `render.output` of its project file, else `renders/<name>.mp4` with the project
/// file's `name`, else the folder's name (`docs/spec/render-command.md` section 6).
fn output_of(project: &Path) -> PathBuf {
    let file = fs::read_to_string(project.join("framesmith.yaml")).unwrap_or_default();
    let value = |key: &str| file.lines().find_map(|line| line.trim().strip_prefix(key).map(|rest| rest.trim().to_string()));
    match value("output:") {
        Some(output) => project.join(output),
        None => project.join("renders").join(format!("{}.mp4", value("name:").unwrap_or_else(|| "demo".to_string()))),
    }
}

#[test]
fn every_entry_of_the_support_table_has_a_fixture() {
    let missing: Vec<String> = with_fixtures()
        .iter()
        .filter(|row| !fixture(row).join("composition.xml").is_file() || !fixture(row).join("expected").is_file())
        .map(|row| row.name.clone())
        .collect();
    assert!(missing.is_empty(), "entries of the support table with no fixture: {}", missing.join(", "));
}

#[test]
fn check_lists_each_entry_on_its_file_and_line_with_its_reason_and_exits_2() {
    let rows = with_fixtures();
    assert!(rows.len() > 60, "section 14 was read: {} rows", rows.len());
    for row in &rows {
        let (project, cwd) = project_of("support-table-check", &row.name, &fixture(row));
        let checked = framesmith(&cwd, ["check", project.to_str().unwrap()], &[("NO_COLOR", "1")]);
        let printed = out(&checked);
        assert_eq!(checked.status.code(), Some(2), "{}: {printed}{}", row.name, err(&checked));
        let (file, line) = expected(row);
        let entry = format!("(support table {}: {}; waits until {})", row.name, row.reason, row.waits);
        assert!(
            printed.lines().any(|printed_line| printed_line
                .starts_with(&format!("{file}, line {line}: FS-"))
                && printed_line.contains(": unsupported: ")
                && printed_line.contains(&entry)),
            "{}: no line names the entry on {file}, line {line}: {printed}",
            row.name
        );
        for report in printed.lines().filter(|printed_line| printed_line.contains(": unsupported: ")) {
            assert!(report.contains("(support table S"), "{}: an unsupported report names no entry: {report}", row.name);
        }
    }
}

#[test]
fn render_prints_the_same_list_exits_2_and_writes_nothing() {
    for row in &with_fixtures() {
        let (project, cwd) = project_of("support-table-check-for-render", &row.name, &fixture(row));
        let checked = framesmith(&cwd, ["check", project.to_str().unwrap()], &[("NO_COLOR", "1")]);
        let (project, cwd) = project_of("support-table-render", &row.name, &fixture(row));
        // A film already at the output path, whose bytes the refused render must keep.
        let film = output_of(&project);
        fs::create_dir_all(film.parent().unwrap()).unwrap();
        let earlier = b"a film made before this run".to_vec();
        fs::write(&film, &earlier).unwrap();
        let before = files_under(&project);
        let rendered = framesmith(&cwd, ["render", project.to_str().unwrap(), "--backend", "cpu"], &[("NO_COLOR", "1")]);
        assert_eq!(rendered.status.code(), Some(2), "{}: {}{}", row.name, out(&rendered), err(&rendered));
        assert_eq!(out(&rendered), out(&checked), "{}: render and check print different lists", row.name);
        assert!(!out(&rendered).contains("Rendered "), "{}: {}", row.name, out(&rendered));
        assert_eq!(fs::read(&film).unwrap(), earlier, "{}: the film at the output path was changed", row.name);
        assert_eq!(files_under(&project), before, "{}: the render left a file in the project", row.name);
        let left: Vec<PathBuf> = files_under(&cwd).into_iter().filter(|path| !path.starts_with("logs")).collect();
        assert!(left.is_empty(), "{}: the render left files where it ran: {left:?}", row.name);
    }
}

#[test]
fn a_project_with_an_unsupported_and_an_invalid_form_lists_both_and_both_commands_exit_1() {
    let from = repository().join("fixtures/refusals-mixed");
    for (command, options) in [("check", &[][..]), ("render", &["--backend", "cpu"][..])] {
        let (project, cwd) = project_of("support-table-mixed", command, &from);
        let mut args = vec![command, project.to_str().unwrap()];
        args.extend(options);
        let run = framesmith(&cwd, args, &[("NO_COLOR", "1")]);
        let printed = out(&run);
        assert_eq!(run.status.code(), Some(1), "{command}: {printed}{}", err(&run));
        let lines: Vec<&str> = printed.lines().collect();
        assert_eq!(lines.len(), 3, "{command}: two reports and the closing line: {printed}");
        assert!(lines[0].starts_with("composition.xml, line 3: FS-0035: unsupported: "), "{command}: {}", lines[0]);
        assert!(lines[0].contains("(support table S36: "), "{command}: {}", lines[0]);
        assert!(lines[1].starts_with("composition.xml, line 4: FS-0030: invalid: "), "{command}: {}", lines[1]);
        assert_eq!(lines[2], "Invalid: the project is not valid (composition.xml)", "{command}");
        assert!(!project.join("renders").exists(), "{command}: a film or its folder was written");
    }
}

#[test]
fn a_form_no_entry_names_is_invalid_and_exits_1() {
    let (project, cwd) = project_of("support-table-unnamed", "check", &repository().join("fixtures/refusals-unnamed"));
    let checked = framesmith(&cwd, ["check", project.to_str().unwrap()], &[("NO_COLOR", "1")]);
    let printed = out(&checked);
    assert_eq!(checked.status.code(), Some(1), "{printed}{}", err(&checked));
    assert!(printed.starts_with("composition.xml, line 3: FS-0039: invalid: "), "{printed}");
    assert!(!printed.contains("support table"), "{printed}");
}
