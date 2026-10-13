//! `framesmith check` on a composition with several mistakes: every problem in one run, each with
//! its code, file and line, in the order of the file, and the exit code.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A project folder `demo` whose `composition.xml` is `text`, next to an empty folder to run from.
fn make_project(name: &str, text: &[u8]) -> (PathBuf, PathBuf) {
    let base = Path::new(env!("CARGO_TARGET_TMPDIR")).join("check-reports").join(name);
    if base.exists() {
        fs::remove_dir_all(&base).unwrap();
    }
    let project = base.join("demo");
    let cwd = base.join("cwd");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&cwd).unwrap();
    fs::write(project.join("composition.xml"), text).unwrap();
    (project, cwd)
}

/// Runs `framesmith check` on the project, from `cwd`, with a terminal that asks for colour.
fn check(project: &Path, cwd: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_framesmith"))
        .arg("check")
        .arg(project)
        .current_dir(cwd)
        .env("CLICOLOR_FORCE", "1")
        .env("FORCE_COLOR", "1")
        .env("TERM", "xterm-256color")
        .env_remove("NO_COLOR")
        .output()
        .unwrap()
}

fn lines(output: &Output) -> Vec<String> {
    String::from_utf8(output.stdout.clone())
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

const INVALID: &str = "Invalid: the project is not valid (composition.xml)";

/// Three wrong root values, on lines 3, 6 and 8; the attributes are not in the order the reader
/// checks them in.
const THREE_WRONG: &str = "<?xml version=\"1.0\"?>\n<composition\n  version=\"1\"\n  id=\"demo\"\n  height=\"360\"\n  background=\"red\"\n  duration=\"2\"\n  width=\"641\"\n  fps=\"0\">\n</composition>\n";

#[test]
fn three_wrong_root_values_are_all_reported_in_one_run_with_code_file_line_and_form_and_exit_1() {
    let (project, cwd) = make_project("three-wrong", THREE_WRONG.as_bytes());
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    assert_eq!(
        lines(&output),
        [
            "composition.xml, line 6: FS-0012: `background=\"red\"` is not a colour written as #rrggbb or #rrggbbaa",
            "composition.xml, line 8: FS-0007: `width` must be an even whole number from 2 to 8192, not 641",
            "composition.xml, line 9: FS-0008: `fps` must be a whole number from 1 to 240, not 0",
            INVALID,
        ]
    );
}

#[test]
fn the_reports_are_printed_in_the_order_of_their_lines_in_the_file() {
    let (project, cwd) = make_project("line-order", THREE_WRONG.as_bytes());
    let reports: Vec<String> = lines(&check(&project, &cwd)).into_iter().filter(|line| line != INVALID).collect();
    let numbers: Vec<u32> = reports
        .iter()
        .map(|report| report.split("line ").nth(1).unwrap().split(':').next().unwrap().parse().unwrap())
        .collect();
    assert_eq!(numbers, [6, 8, 9]);
    assert!(numbers.windows(2).all(|pair| pair[0] < pair[1]));

    // The same three mistakes in the reader's own order of checks give the lines in that order.
    let other = "<composition\n  version=\"1\"\n  width=\"641\"\n  height=\"360\"\n  fps=\"0\"\n  duration=\"2\"\n  background=\"red\"/>";
    let (project, cwd) = make_project("line-order-checks", other.as_bytes());
    let text = lines(&check(&project, &cwd));
    let codes: Vec<&str> = text
        .iter()
        .filter_map(|line| line.split(": ").nth(1))
        .filter(|c| c.starts_with("FS-"))
        .collect();
    assert_eq!(codes, ["FS-0007", "FS-0008", "FS-0012"]);
}

#[test]
fn a_file_that_is_not_well_formed_xml_is_reported_once_with_the_readers_reason_and_line() {
    // The root is also wrong in three ways; none of them is reported.
    let text = "<composition version=\"2\" width=\"641\" duration=\"0\">\n<box>\n</composition>\n";
    let (project, cwd) = make_project("not-xml", text.as_bytes());
    let output = check(&project, &cwd);
    assert_eq!(output.status.code(), Some(1));
    let printed = lines(&output);
    assert_eq!(printed.len(), 2, "{printed:?}");
    assert!(
        printed[0].starts_with("composition.xml, line 3: FS-0002: the file is not well-formed XML: "),
        "{}",
        printed[0]
    );
    assert!(printed[0].len() > "composition.xml, line 3: FS-0002: the file is not well-formed XML: ".len());
    assert_eq!(printed[1], INVALID);

    // Not UTF-8 text is also reported once.
    let mut bytes = b"<composition version=\"2\"\n id=\"".to_vec();
    bytes.push(0xff);
    bytes.extend_from_slice(b"\"/>");
    let (project, cwd) = make_project("not-utf-8", &bytes);
    let printed = lines(&check(&project, &cwd));
    assert_eq!(
        printed,
        [
            "composition.xml, line 2: FS-0001: the file is not well-formed XML: it is not valid UTF-8 text",
            INVALID
        ]
    );
}

#[test]
fn after_the_author_corrects_every_reported_line_check_exits_0() {
    let (project, cwd) = make_project("corrected", THREE_WRONG.as_bytes());
    let first = check(&project, &cwd);
    assert_eq!(first.status.code(), Some(1));
    // Correct exactly the lines the run reported, and nothing else.
    let reported = lines(&first).len() - 1;
    assert_eq!(reported, 3);
    let corrected: Vec<String> = THREE_WRONG
        .lines()
        .enumerate()
        .map(|(index, line)| match index + 1 {
            6 => "  background=\"#ff0000\"".to_string(),
            8 => "  width=\"640\"".to_string(),
            9 => "  fps=\"24\">".to_string(),
            _ => line.to_string(),
        })
        .collect();
    fs::write(project.join("composition.xml"), corrected.join("\n")).unwrap();
    let second = check(&project, &cwd);
    assert_eq!(second.status.code(), Some(0));
    assert_eq!(lines(&second), ["Ready: the project is valid, 48 frames, 640x360, 24 fps"]);
}

#[test]
fn each_report_is_one_line_that_names_its_file_and_there_is_no_colour_with_one_file_read() {
    let (project, cwd) = make_project("plain", THREE_WRONG.as_bytes());
    let output = check(&project, &cwd);
    assert!(!output.stdout.contains(&0x1b) && !output.stderr.contains(&0x1b));
    assert!(output.stdout.iter().all(|&byte| byte == b'\n' || (0x20..0x7f).contains(&byte)));
    let printed = lines(&output);
    let reports = &printed[..printed.len() - 1];
    assert_eq!(reports.len(), 3);
    for report in reports {
        // The file, the line, the code: the same three parts on every report, in the same places.
        let (place, rest) = report.split_once(": ").unwrap();
        let (file, line) = place.split_once(", line ").unwrap();
        assert_eq!(file, "composition.xml");
        assert!(line.parse::<u32>().is_ok(), "{report}");
        assert!(
            rest.starts_with("FS-") && rest[3..7].bytes().all(|b| b.is_ascii_digit()),
            "{report}"
        );
        assert_eq!(&rest[7..9], ": ", "{report}");
    }
    // A report ends where the line does: the next one starts a line of its own.
    assert_eq!(output.stdout.iter().filter(|&&byte| byte == b'\n').count(), printed.len());
}
