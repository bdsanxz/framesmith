//! The compositions a composition shows with `<composition src>`, judged when the project is opened
//! (`docs/spec/notation.md` section 10): a chain of files and its depth, loops, frame rates, missing and
//! unreadable files, the styles of each file, and the problems of the shown files reported in one run.

use project::{Code, OpenError, Project, Report, MAX_CHAIN_DEPTH};
use std::fs;
use std::path::{Path, PathBuf};

/// A new folder named `name` for one test, under the tests' scratch folder.
fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("shown-compositions").join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A composition of 96 by 54 pixels, two seconds long, at `fps` frames a second, with `inside` on its
/// second line: the lines of the root are 1 for the root's own tag and 2 for what `inside` holds.
fn composition(fps: u32, inside: &str) -> String {
    format!("<composition version=\"1\" width=\"96\" height=\"54\" fps=\"{fps}\" duration=\"2\">\n{inside}\n</composition>\n")
}

/// Writes `text` as the file `name` of the project folder `dir`.
fn write(dir: &Path, name: &str, text: &str) {
    fs::write(dir.join(name), text).unwrap();
}

/// The reports of a project that does not open, in the order the command prints them.
fn reports_of(dir: &Path) -> Vec<Report> {
    match Project::open(dir) {
        Err(OpenError::Invalid(reports)) => reports,
        Err(other) => panic!("expected the problems of the files, got {other:?}"),
        Ok(_) => panic!("expected the project not to open"),
    }
}

/// The message of the one report of `file` on `line`, with `code`. Panics with every report when there is none.
fn message_of(reports: &[Report], file: &str, line: u32, code: Code) -> String {
    reports
        .iter()
        .find(|report| report.file() == file && report.problem().line() == line && report.problem().code() == code)
        .map(|report| report.problem().message().to_string())
        .unwrap_or_else(|| panic!("no {code} on line {line} of {file} in {reports:?}"))
}

#[test]
fn a_chain_of_sixteen_files_opens_with_each_file_read_once() {
    let dir = folder("chain-of-sixteen");
    // The root shows f2, and each file shows the next: sixteen files in all, the root counted.
    write(&dir, "composition.xml", &composition(30, "<composition src=\"f2.xml\"/>"));
    for number in 2..MAX_CHAIN_DEPTH {
        let text = composition(30, &format!("<composition src=\"f{}.xml\"/>", number + 1));
        write(&dir, &format!("f{number}.xml"), &text);
    }
    write(&dir, &format!("f{MAX_CHAIN_DEPTH}.xml"), &composition(30, ""));
    let project = Project::open(&dir).unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(
        project.shown_files().len(),
        MAX_CHAIN_DEPTH - 1,
        "the files shown, the root not counted"
    );
    assert!(project.shown_file("f16.xml").is_some(), "the last file of the chain is shown");
}

#[test]
fn a_chain_of_seventeen_files_is_reported_where_the_depth_is_passed() {
    let dir = folder("chain-of-seventeen");
    write(&dir, "composition.xml", &composition(30, "<composition src=\"f2.xml\"/>"));
    for number in 2..MAX_CHAIN_DEPTH {
        let text = composition(30, &format!("<composition src=\"f{}.xml\"/>", number + 1));
        write(&dir, &format!("f{number}.xml"), &text);
    }
    // The element in the sixteenth file names the seventeenth file, which the chain may not hold.
    write(
        &dir,
        &format!("f{MAX_CHAIN_DEPTH}.xml"),
        &composition(30, "<composition src=\"f17.xml\"/>"),
    );
    let reports = reports_of(&dir);
    let message = message_of(&reports, "f16.xml", 2, Code::ShownTooDeep);
    assert!(message.contains("17th"), "the depth is named: {message}");
    assert_eq!(reports.len(), 1, "one report for the one element: {reports:?}");
}

#[test]
fn a_file_that_shows_itself_is_reported_with_the_loop() {
    let dir = folder("shows-itself");
    write(&dir, "composition.xml", &composition(30, "<composition src=\"a.xml\"/>"));
    write(&dir, "a.xml", &composition(30, "<composition src=\"a.xml\"/>"));
    let reports = reports_of(&dir);
    let message = message_of(&reports, "a.xml", 2, Code::ShownLoop);
    assert!(message.contains("the loop is `a.xml`, `a.xml`"), "the files of the loop: {message}");
    assert_eq!(reports.len(), 1, "{reports:?}");
}

#[test]
fn a_file_that_shows_itself_through_another_file_is_reported_with_the_files_of_the_loop() {
    let dir = folder("shows-through-another");
    write(&dir, "composition.xml", &composition(30, "<composition src=\"b.xml\"/>"));
    write(&dir, "b.xml", &composition(30, "<composition src=\"c.xml\"/>"));
    // Line 3 of c.xml names b.xml, which is on the chain already.
    write(&dir, "c.xml", "<composition version=\"1\" width=\"96\" height=\"54\" fps=\"30\" duration=\"2\">\n<box left=\"0\" top=\"0\" width=\"4\" height=\"4\"/>\n<composition src=\"b.xml\"/>\n</composition>\n");
    let reports = reports_of(&dir);
    let message = message_of(&reports, "c.xml", 3, Code::ShownLoop);
    assert!(message.contains("the loop is `b.xml`, `c.xml`, `b.xml`"), "{message}");
    assert_eq!(reports.len(), 1, "{reports:?}");
}

#[test]
fn a_shown_file_with_another_frame_rate_is_reported_with_both_rates_and_the_element_line() {
    let dir = folder("other-frame-rate");
    write(&dir, "composition.xml", &composition(30, "<composition src=\"a.xml\"/>"));
    write(&dir, "a.xml", &composition(25, ""));
    let reports = reports_of(&dir);
    let message = message_of(&reports, "composition.xml", 2, Code::ShownFpsDiffers);
    assert!(message.contains("25 frames a second"), "the rate of the shown file: {message}");
    assert!(message.contains("has 30"), "the rate of the composition that shows it: {message}");
    assert_eq!(reports.len(), 1, "{reports:?}");
}

#[test]
fn a_shown_file_that_writes_no_frame_rate_has_thirty_and_differs_from_another_rate() {
    let dir = folder("default-frame-rate");
    write(&dir, "composition.xml", &composition(25, "<composition src=\"a.xml\"/>"));
    write(
        &dir,
        "a.xml",
        "<composition version=\"1\" width=\"96\" height=\"54\" duration=\"2\">\n</composition>\n",
    );
    let reports = reports_of(&dir);
    let message = message_of(&reports, "composition.xml", 2, Code::ShownFpsDiffers);
    assert!(message.contains("30 frames a second"), "{message}");
    assert_eq!(reports.len(), 1, "{reports:?}");
}

#[test]
fn a_shown_file_that_is_missing_is_reported_with_its_line_and_the_path_looked_for() {
    let dir = folder("missing");
    write(&dir, "composition.xml", &composition(30, "<composition src=\"scenes/gone.xml\"/>"));
    let reports = reports_of(&dir);
    let message = message_of(&reports, "composition.xml", 2, Code::ShownMissing);
    assert!(message.contains("`scenes/gone.xml`"), "the path looked for: {message}");
    assert_eq!(reports.len(), 1, "{reports:?}");
}

#[cfg(unix)]
#[test]
fn a_shown_file_that_cannot_be_read_is_reported_with_the_reason() {
    use std::os::unix::fs::PermissionsExt;
    let dir = folder("unreadable");
    write(&dir, "composition.xml", &composition(30, "<composition src=\"locked.xml\"/>"));
    write(&dir, "locked.xml", &composition(30, ""));
    let path = dir.join("locked.xml");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
    let outcome = Project::open(&dir);
    // Give the file its permission back before anything can fail, so that the folder can be removed.
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let reports = match outcome {
        Err(OpenError::Invalid(reports)) => reports,
        other => panic!("expected the file to be reported, got {other:?}"),
    };
    let message = message_of(&reports, "composition.xml", 2, Code::ShownUnreadable);
    assert!(message.contains("`locked.xml`"), "{message}");
    assert_eq!(reports.len(), 1, "{reports:?}");
}

#[test]
fn a_style_of_the_composition_is_not_taken_by_an_element_of_a_shown_file() {
    let dir = folder("style-not-taken-inside");
    write(
        &dir,
        "composition.xml",
        "<composition version=\"1\" width=\"96\" height=\"54\" fps=\"30\" duration=\"2\">\n\
         <style id=\"accent\" background=\"#e0a030\"/>\n\
         <composition src=\"a.xml\"/>\n\
         </composition>\n",
    );
    write(
        &dir,
        "a.xml",
        "<composition version=\"1\" width=\"96\" height=\"54\" fps=\"30\" duration=\"2\">\n\
         <box class=\"accent\" left=\"0\" top=\"0\" width=\"4\" height=\"4\"/>\n\
         </composition>\n",
    );
    let reports = reports_of(&dir);
    let message = message_of_any(&reports, "a.xml", 2);
    assert!(message.contains("accent"), "the class that names the style: {message}");
    assert_eq!(reports.len(), 1, "{reports:?}");
}

#[test]
fn a_style_of_a_shown_file_is_not_taken_by_an_element_of_the_composition() {
    let dir = folder("style-not-taken-outside");
    write(
        &dir,
        "composition.xml",
        "<composition version=\"1\" width=\"96\" height=\"54\" fps=\"30\" duration=\"2\">\n\
         <composition src=\"a.xml\"/>\n\
         <box class=\"hint\" left=\"0\" top=\"0\" width=\"4\" height=\"4\"/>\n\
         </composition>\n",
    );
    write(
        &dir,
        "a.xml",
        "<composition version=\"1\" width=\"96\" height=\"54\" fps=\"30\" duration=\"2\">\n\
         <style id=\"hint\" background=\"#40c0ff\"/>\n\
         <box class=\"hint\" left=\"0\" top=\"0\" width=\"4\" height=\"4\"/>\n\
         </composition>\n",
    );
    let reports = reports_of(&dir);
    let message = message_of_any(&reports, "composition.xml", 3);
    assert!(message.contains("hint"), "the class that names the style: {message}");
    assert_eq!(reports.len(), 1, "{reports:?}");
}

#[test]
fn a_shown_file_takes_the_styles_of_its_own_file() {
    let dir = folder("own-styles");
    write(&dir, "composition.xml", &composition(30, "<composition src=\"a.xml\"/>"));
    write(
        &dir,
        "a.xml",
        "<composition version=\"1\" width=\"96\" height=\"54\" fps=\"30\" duration=\"2\">\n\
         <style id=\"accent\" background=\"#e0a030\"/>\n\
         <box class=\"accent\" left=\"0\" top=\"0\" width=\"4\" height=\"4\"/>\n\
         </composition>\n",
    );
    let project = Project::open(&dir).unwrap_or_else(|error| panic!("{error:?}"));
    assert!(project.shown_file("a.xml").is_some());
}

#[test]
fn problems_of_a_shown_file_and_of_the_composition_are_reported_in_one_run() {
    let dir = folder("one-run");
    // The root and inset.xml are both 641 pixels wide, which is odd, and inset.xml shows a file that is missing:
    // all three are reported in one run, each with the name of its file and its line.
    write(
        &dir,
        "composition.xml",
        "<composition version=\"1\" width=\"641\" height=\"54\" fps=\"30\" duration=\"2\">\n\
         <composition src=\"inset.xml\"/>\n\
         </composition>\n",
    );
    write(
        &dir,
        "inset.xml",
        "<composition version=\"1\" width=\"641\" height=\"54\" fps=\"30\" duration=\"2\">\n\
         <composition src=\"gone.xml\"/>\n\
         </composition>\n",
    );
    let reports = reports_of(&dir);
    let places: Vec<(&str, u32)> = reports.iter().map(|report| (report.file(), report.problem().line())).collect();
    assert_eq!(
        places,
        vec![("composition.xml", 1), ("inset.xml", 1), ("inset.xml", 2)],
        "the reports of the root, then of the file it shows, each on its line"
    );
}

#[test]
fn a_shown_file_shown_twice_has_its_problems_reported_once() {
    let dir = folder("shown-twice");
    write(
        &dir,
        "composition.xml",
        &composition(30, "<composition src=\"a.xml\"/>\n<composition src=\"a.xml\"/>"),
    );
    // The width of a.xml is 641 pixels, which is odd: a problem of the file's root, on line 1.
    write(
        &dir,
        "a.xml",
        "<composition version=\"1\" width=\"641\" height=\"54\" fps=\"30\" duration=\"2\">\n\
         <box left=\"0\" top=\"0\" width=\"4\" height=\"4\"/>\n\
         </composition>\n",
    );
    let reports = reports_of(&dir);
    assert_eq!(reports.len(), 1, "one report for a file shown twice: {reports:?}");
    assert_eq!(reports[0].file(), "a.xml");
    assert_eq!(reports[0].problem().line(), 1);
}

/// The message of the one report of `file` on `line`, whatever its code.
fn message_of_any(reports: &[Report], file: &str, line: u32) -> String {
    reports
        .iter()
        .find(|report| report.file() == file && report.problem().line() == line)
        .map(|report| report.problem().message().to_string())
        .unwrap_or_else(|| panic!("no report on line {line} of {file} in {reports:?}"))
}
