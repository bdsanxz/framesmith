//! The list of codes of `docs/spec/error-catalogue.md`: what it must hold for every code, and how
//! the program is held to it. Each check is a function over the text of the list, so that a test
//! can show it fails, and names the code, on a list that is wrong.

use notation::{read_root, Code, Kind};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// The root of the repository.
fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn read(relative: &str) -> String {
    let path = repository().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

const SPEC: &str = "docs/spec/error-catalogue.md";
const RECORD: &str = "docs/spec/error-catalogue-record.md";

/// One row of a table: the cells between the vertical bars, with the white space removed.
fn cells(line: &str) -> Option<Vec<String>> {
    let line = line.trim();
    if !line.starts_with('|') || !line.ends_with('|') {
        return None;
    }
    let inner = &line[1..line.len() - 1];
    Some(inner.split('|').map(|cell| cell.trim().to_string()).collect())
}

/// The rows of the first table under the heading that begins with `heading` (of the whole text, when
/// `heading` is empty), without the header and the separator.
fn table_under(text: &str, heading: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    // An empty heading means the whole text.
    let mut inside = heading.is_empty();
    let mut seen_table = false;
    for line in text.lines() {
        if line.starts_with("## ") {
            if inside {
                break;
            }
            inside = line.starts_with(heading);
            continue;
        }
        if !inside {
            continue;
        }
        match cells(line) {
            Some(row) => {
                seen_table = true;
                rows.push(row);
            }
            None if seen_table => break,
            None => {}
        }
    }
    assert!(rows.len() >= 2, "no table under {heading:?}");
    rows.drain(..2);
    rows
}

/// A row of the list of codes.
#[derive(Clone, Debug)]
struct Row {
    code: String,
    kind: String,
    meaning: String,
    remedy: String,
}

/// The rows of the list of codes in the text of the catalogue.
fn list_of(text: &str) -> Vec<Row> {
    table_under(text, "## 4.")
        .into_iter()
        .map(|cells| {
            assert_eq!(cells.len(), 4, "a row of the list has four cells: {cells:?}");
            Row {
                code: cells[0].trim_matches('`').to_string(),
                kind: cells[1].clone(),
                meaning: cells[2].clone(),
                remedy: cells[3].clone(),
            }
        })
        .collect()
}

/// The recorded meanings: code, then the meaning when it was recorded.
fn record_of(text: &str) -> Vec<(String, String)> {
    table_under(text, "")
        .into_iter()
        .map(|cells| {
            assert_eq!(cells.len(), 2, "a row of the record has two cells: {cells:?}");
            (cells[0].trim_matches('`').to_string(), cells[1].clone())
        })
        .collect()
}

/// `FS-` and four digits, and nothing else.
fn is_a_code(text: &str) -> bool {
    text.len() == 7 && text.starts_with("FS-") && text[3..].bytes().all(|byte| byte.is_ascii_digit())
}

/// One sentence: it starts with a capital letter, ends with a full stop, and has no other full
/// stop followed by a space.
fn is_one_sentence(text: &str) -> bool {
    text.chars().next().is_some_and(char::is_uppercase) && text.ends_with('.') && !text.contains(". ")
}

/// What is wrong with the rows of a list, each problem naming its code.
fn walk(rows: &[Row]) -> Vec<String> {
    let kinds: Vec<&str> = Kind::ALL.iter().map(|kind| kind.as_str()).collect();
    let mut problems = Vec::new();
    for row in rows {
        let code = &row.code;
        if !is_a_code(code) {
            problems.push(format!("{code}: a code is FS- and four digits"));
        }
        if row.kind.is_empty() {
            problems.push(format!("{code}: no kind"));
        } else if !kinds.contains(&row.kind.as_str()) {
            problems.push(format!("{code}: the kind {:?} is not one of {kinds:?}", row.kind));
        }
        if row.meaning.is_empty() {
            problems.push(format!("{code}: no meaning"));
        }
        if row.remedy.is_empty() {
            problems.push(format!("{code}: no remedy sentence"));
        } else if !is_one_sentence(&row.remedy) {
            problems.push(format!("{code}: the remedy is not one sentence: {:?}", row.remedy));
        }
    }
    let mut seen = BTreeSet::new();
    for row in rows {
        if !seen.insert(&row.code) {
            problems.push(format!("{}: listed twice", row.code));
        }
    }
    problems
}

/// Every `FS-` and four digits written in a line of the source that is not a comment, with the
/// file it is in.
fn codes_in_source() -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    let mut crates = Vec::new();
    for entry in fs::read_dir(repository().join("crates")).unwrap() {
        crates.push(entry.unwrap().path().join("src"));
    }
    let mut pending: Vec<PathBuf> = crates.into_iter().filter(|dir| dir.is_dir()).collect();
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                for line in fs::read_to_string(&path).unwrap().lines() {
                    if line.trim_start().starts_with("//") {
                        continue;
                    }
                    for (at, _) in line.match_indices("FS-") {
                        if let Some(candidate) = line.get(at..at + 7) {
                            if is_a_code(candidate) {
                                found.insert(candidate.to_string(), path.display().to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    found
}

/// The codes the program can emit: the list of the `Code` type and every code its source writes.
fn program_codes() -> BTreeSet<String> {
    let mut codes: BTreeSet<String> = Code::ALL.iter().map(|code| code.as_str().to_string()).collect();
    codes.extend(codes_in_source().into_keys());
    codes
}

/// The differences between the codes the program can emit and the codes of the list, each naming
/// the code.
fn compare_codes(program: &BTreeSet<String>, listed: &BTreeSet<String>) -> Vec<String> {
    let mut problems = Vec::new();
    for code in program.difference(listed) {
        problems.push(format!("{code}: the program can emit it and the spec does not list it"));
    }
    for code in listed.difference(program) {
        problems.push(format!("{code}: the spec lists it and the program cannot emit it"));
    }
    problems
}

/// Where the meaning of a code in the list differs from the one recorded, each naming the code.
fn meaning_drift(rows: &[Row], record: &[(String, String)]) -> Vec<String> {
    let mut problems = Vec::new();
    for (code, recorded) in record {
        match rows.iter().find(|row| &row.code == code) {
            None => problems.push(format!("{code}: recorded and no longer on the list")),
            Some(row) if &row.meaning != recorded => problems.push(format!(
                "{code}: the meaning was recorded as {recorded:?} and the list says {:?}",
                row.meaning
            )),
            Some(_) => {}
        }
    }
    problems
}

fn listed_codes(rows: &[Row]) -> BTreeSet<String> {
    rows.iter().map(|row| row.code.clone()).collect()
}

fn the_list() -> Vec<Row> {
    list_of(&read(SPEC))
}

#[test]
fn the_list_holds_a_row_for_every_code_of_the_program_and_every_code_begins_fs() {
    let rows = the_list();
    assert_eq!(rows.len(), Code::ALL.len());
    for row in &rows {
        assert!(row.code.starts_with("FS-"), "{}", row.code);
        assert!(is_a_code(&row.code), "{}", row.code);
    }
    let order: Vec<&str> = rows.iter().map(|row| row.code.as_str()).collect();
    let expected: Vec<&str> = Code::ALL.iter().map(|code| code.as_str()).collect();
    assert_eq!(order, expected, "the rows are in the order of the numbers and of `Code::ALL`");
}

#[test]
fn every_code_of_the_list_has_a_kind_a_meaning_and_a_remedy_sentence() {
    let problems = walk(&the_list());
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn a_code_with_no_remedy_sentence_fails_the_walk_and_is_named() {
    let text = read(SPEC).replace("| Write the size as an even whole number from 2 to 8192. |", "|  |");
    let problems = walk(&list_of(&text));
    assert_eq!(problems, ["FS-0007: no remedy sentence"]);

    let text = read(SPEC).replace(
        "| Save the file as UTF-8 text. |",
        "| Save the file as UTF-8 text and try again. Or give up. |",
    );
    let problems = walk(&list_of(&text));
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].starts_with("FS-0001: the remedy is not one sentence"), "{problems:?}");
}

#[test]
fn a_code_with_no_kind_or_an_unknown_kind_fails_the_walk_and_is_named() {
    let text = read(SPEC).replace("| `FS-0008` | invalid |", "| `FS-0008` |  |");
    assert_eq!(walk(&list_of(&text)), ["FS-0008: no kind"]);

    let text = read(SPEC).replace("| `FS-0009` | invalid |", "| `FS-0009` | broken |");
    let problems = walk(&list_of(&text));
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].starts_with("FS-0009: the kind \"broken\" is not one of"),
        "{problems:?}"
    );
}

#[test]
fn a_code_that_does_not_begin_fs_or_is_listed_twice_fails_the_walk_and_is_named() {
    let text = read(SPEC).replace("| `FS-0010` |", "| `XX-0010` |");
    assert_eq!(walk(&list_of(&text)), ["XX-0010: a code is FS- and four digits"]);

    let text = read(SPEC).replace("| `FS-0011` |", "| `FS-0010` |");
    assert_eq!(walk(&list_of(&text)), ["FS-0010: listed twice"]);
}

#[test]
fn the_codes_the_program_can_emit_are_the_codes_of_the_list() {
    let problems = compare_codes(&program_codes(), &listed_codes(&the_list()));
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_source_names_exactly_the_codes_of_the_code_type() {
    // A variant that is missing from `Code::ALL` would be written in `as_str` and be found here.
    let from_source: BTreeSet<String> = codes_in_source().into_keys().collect();
    let from_all: BTreeSet<String> = Code::ALL.iter().map(|code| code.as_str().to_string()).collect();
    assert_eq!(from_source, from_all);
    assert_eq!(from_all.len(), Code::ALL.len(), "a code appears twice in `Code::ALL`");
}

#[test]
fn a_code_the_program_can_emit_and_the_list_lacks_fails_and_is_named() {
    let mut listed = listed_codes(&the_list());
    listed.remove("FS-0012");
    assert_eq!(
        compare_codes(&program_codes(), &listed),
        ["FS-0012: the program can emit it and the spec does not list it"]
    );
}

#[test]
fn a_code_the_list_has_and_the_program_cannot_emit_fails_and_is_named() {
    let mut listed = listed_codes(&the_list());
    listed.insert("FS-0099".to_string());
    assert_eq!(
        compare_codes(&program_codes(), &listed),
        ["FS-0099: the spec lists it and the program cannot emit it"]
    );
}

#[test]
fn the_kind_and_the_remedy_of_every_code_in_the_program_are_those_of_the_list() {
    let rows = the_list();
    for code in Code::ALL {
        let row = rows
            .iter()
            .find(|row| row.code == code.as_str())
            .unwrap_or_else(|| panic!("{code} is not listed"));
        assert_eq!(code.kind().as_str(), row.kind, "{code}");
        assert_eq!(code.remedy(), row.remedy, "{code}");
    }
}

#[test]
fn the_kinds_of_the_program_are_the_three_words_of_the_spec() {
    let table = table_under(&read(SPEC), "## 2.");
    let printed: Vec<String> = table.iter().map(|cells| cells[1].trim_matches('`').to_string()).collect();
    let program: Vec<String> = Kind::ALL.iter().map(|kind| kind.as_str().to_string()).collect();
    assert_eq!(printed, ["invalid", "unsupported", "failure"]);
    assert_eq!(program, printed);
}

#[test]
fn the_meaning_of_every_code_is_the_one_recorded() {
    let record = record_of(&read(RECORD));
    assert!(!record.is_empty());
    let problems = meaning_drift(&the_list(), &record);
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn every_code_of_the_list_has_a_row_in_the_record() {
    // The record holds the codes of the list as it stands; a code added later is added to it when
    // the code is first merged (the spec, section 5).
    let recorded: BTreeSet<String> = record_of(&read(RECORD)).into_iter().map(|(code, _)| code).collect();
    assert_eq!(recorded, listed_codes(&the_list()));
}

#[test]
fn a_meaning_that_differs_from_the_recorded_one_fails_and_names_the_code() {
    let record = record_of(&read(RECORD));
    let text = read(SPEC).replace("The root's `duration` is 0.", "The root's `duration` is zero or less.");
    let problems = meaning_drift(&list_of(&text), &record);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].starts_with("FS-0011: the meaning was recorded as"), "{problems:?}");
}

#[test]
fn a_recorded_code_that_is_gone_from_the_list_fails_and_names_the_code() {
    let record = record_of(&read(RECORD));
    let mut rows = the_list();
    rows.retain(|row| row.code != "FS-0003");
    assert_eq!(meaning_drift(&rows, &record), ["FS-0003: recorded and no longer on the list"]);
}

#[test]
fn a_code_given_another_meaning_in_the_record_is_caught_the_same_way() {
    // The record itself is the other side: a recorded meaning that is changed no longer matches.
    let text = read(RECORD).replace("The composition file is not valid UTF-8 text.", "The file is empty.");
    let problems = meaning_drift(&the_list(), &record_of(&text));
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].starts_with("FS-0001:"), "{problems:?}");
}

#[test]
fn a_problem_prints_its_code_its_kind_its_message_and_the_remedy_of_the_list_as_its_last_sentence() {
    let rows = the_list();
    let cases: [(&str, &str); 4] = [
        (
            "<composition width=\"641\" height=\"360\" duration=\"2\" version=\"1\"/>",
            "FS-0007",
        ),
        ("<composition width=\"640\" height=\"360\" duration=\"2\"/>", "FS-0004"),
        (
            "<composition version=\"1\" width=\"640\" height=\"360\" duration=\"0\"/>",
            "FS-0011",
        ),
        (
            "<composition version=\"1\" width=\"640\" height=\"360\" duration=\"2\" background=\"red\"/>",
            "FS-0012",
        ),
    ];
    for (source, expected) in cases {
        let problems = read_root(source.as_bytes()).unwrap_err();
        assert_eq!(problems.len(), 1, "{source}");
        let problem = &problems[0];
        let row = rows.iter().find(|row| row.code == expected).unwrap();
        assert_eq!(problem.code().as_str(), expected);
        assert_eq!(problem.kind().as_str(), row.kind);
        assert_eq!(problem.remedy(), row.remedy);
        let line = problem.to_string();
        let prefix = format!("line 1: {expected}: {}: {}", row.kind, problem.message());
        assert!(line.starts_with(&prefix), "{line}");
        assert!(line.ends_with(&format!(" {}", row.remedy)), "{line}");
        assert!(!line.contains('\n'), "{line}");
    }
}

#[test]
fn a_message_that_ends_with_a_full_stop_gets_no_second_one() {
    let statement = Code::NotXml.statement("the file is not well-formed XML: unexpected end.");
    assert_eq!(
        statement,
        format!(
            "FS-0002: invalid: the file is not well-formed XML: unexpected end. {}",
            Code::NotXml.remedy()
        )
    );
    let statement = Code::NotXml.statement("the file is not well-formed XML");
    assert!(
        statement.starts_with("FS-0002: invalid: the file is not well-formed XML. "),
        "{statement}"
    );
}
