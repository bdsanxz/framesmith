//! `framesmith render` on a machine with no graphics card, run in the process with the card's starter
//! given by the test, so that no card is needed and no machine is changed (`docs/spec/render-command.md`
//! sections 1.2 and 7; `docs/spec/card-renderer.md` section 2). `gpu` stops before the first frame with
//! the card's reason, the setting to change and exit code 2, and writes no file. `auto` renders on the
//! processor and says so. The run's log holds the codes and the outcome, never the reason.

mod common;

use std::ffi::OsString;
use std::path::Path;
use std::sync::Mutex;

use common::lines;
use common::log_text;
use common::project as make;
use draw::{DisplayList, Error, Picture, Renderer};
use draw_cpu::CpuRenderer;
use measure::video_facts;

const GROUP: &str = "render-without-a-card";

const FILM: &str = "\
<composition version=\"1\" id=\"film\" width=\"64\" height=\"48\" fps=\"10\" duration=\"0.5\" background=\"#204060\">
<box left=\"8\" top=\"8\" width=\"16\" height=\"16\" background=\"#ffffff\"/>
</composition>
";

const NO_CARD: &str = "this machine has no graphics card frx7 can draw with: no adapter was found";

/// `run_with` changes the working folder, which is one for the whole process: the tests take turns.
static TURNS: Mutex<()> = Mutex::new(());

/// A renderer that stands for the card on a machine that has one: it draws as the processor does.
struct FakeCard(CpuRenderer);

impl Renderer for FakeCard {
    fn name(&self) -> &'static str {
        "card"
    }

    fn draw(&mut self, list: &DisplayList) -> Result<Picture, Error> {
        self.0.draw(list)
    }
}

fn card_starts() -> Result<Box<dyn Renderer>, Error> {
    Ok(Box::new(FakeCard(CpuRenderer)))
}

fn card_does_not_start() -> Result<Box<dyn Renderer>, Error> {
    Err(Error::new(NO_CARD))
}

fn card_must_not_start() -> Result<Box<dyn Renderer>, Error> {
    panic!("the card must not be started for this setting")
}

/// Runs `framesmith render <project> <options>` in the process, with the card's starter given, and
/// returns the exit code, what was printed, and what was printed to the error output.
fn run(cwd: &Path, project: &Path, options: &[&str], start_card: &cli::StartCard) -> (u8, String, String) {
    let _turn = TURNS.lock().unwrap_or_else(|poison| poison.into_inner());
    std::env::set_current_dir(cwd).unwrap();
    let mut args: Vec<OsString> = vec!["render".into(), project.as_os_str().to_owned()];
    args.extend(options.iter().map(OsString::from));
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let code = cli::run_with(&args, &mut out, &mut err, start_card);
    (code, String::from_utf8(out).unwrap(), String::from_utf8(err).unwrap())
}

#[test]
fn gpu_on_a_machine_with_no_card_stops_before_the_first_frame_with_the_reason_and_writes_no_file() {
    let (project, cwd) = make(GROUP, "gpu-refused", FILM);
    let (code, printed, told) = run(&cwd, &project, &["--backend", "gpu"], &card_does_not_start);

    assert_eq!(code, 2, "{printed}{told}");
    assert_eq!(told, "", "{printed}{told}");
    let printed_lines: Vec<&str> = printed.lines().collect();
    assert_eq!(printed_lines.len(), 2, "{printed}");
    assert!(
        printed_lines[0].starts_with(&format!("FS-0038: unsupported: {NO_CARD}.")),
        "{printed}"
    );
    assert!(
        printed_lines[0]
            .ends_with("Set `render.backend` (or `--backend`) to `auto` or `cpu`, which draw on the processor when no card is there."),
        "{printed}"
    );
    assert_eq!(
        printed_lines[1], "Refused: the video was not made, because the graphics card that `gpu` needs is not available",
        "{printed}"
    );

    assert!(!project.join("renders").exists(), "a folder was made for a film that was refused");
    assert!(
        !project.join("renders").join("demo.mp4").exists(),
        "a file was written for a film that was refused"
    );

    let log_lines = lines(&cwd);
    let refused = log_lines
        .iter()
        .find(|line| line.component == "cli" && line.text.contains("outcome=\"refused\""))
        .unwrap_or_else(|| panic!("no refused line in the log: {}", log_text(&cwd)));
    assert_eq!(refused.level, "WARN", "{}", log_text(&cwd));
    assert!(
        refused.text.contains("outcome=\"refused\"") && refused.text.contains("code=\"FS-0038\""),
        "{}",
        refused.text
    );
    assert!(
        !log_text(&cwd).contains("no adapter"),
        "the reason of the card is in the log: {}",
        log_text(&cwd)
    );
}

#[test]
fn auto_on_a_machine_with_no_card_renders_on_the_processor_and_the_summary_says_so() {
    let (project, cwd) = make(GROUP, "auto-no-card", FILM);
    let (code, printed, told) = run(&cwd, &project, &[], &card_does_not_start);

    assert_eq!(code, 0, "{printed}{told}");
    assert_eq!(told, "", "{printed}{told}");
    let printed_lines: Vec<&str> = printed.lines().collect();
    assert_eq!(printed_lines.len(), 14, "{printed}");
    assert_eq!(
        printed_lines[5], "renderer: processor (chosen by auto: no suitable card)",
        "{printed}"
    );
    assert_eq!(printed_lines[7], "backend: auto (default)", "{printed}");

    let film = project.join("renders").join("demo.mp4");
    assert_eq!(video_facts(&film).unwrap().frames, 5, "{printed}");
}

#[test]
fn auto_on_a_machine_with_a_card_names_the_card_and_says_it_was_chosen_by_auto() {
    let (project, cwd) = make(GROUP, "auto-card", FILM);
    let (code, printed, told) = run(&cwd, &project, &[], &card_starts);

    assert_eq!(code, 0, "{printed}{told}");
    let printed_lines: Vec<&str> = printed.lines().collect();
    assert_eq!(printed_lines[5], "renderer: card (chosen by auto)", "{printed}");
}

#[test]
fn cpu_never_starts_the_card_in_a_run() {
    let (project, cwd) = make(GROUP, "cpu-never-starts", FILM);
    let (code, printed, told) = run(&cwd, &project, &["--backend", "cpu"], &card_must_not_start);

    assert_eq!(code, 0, "{printed}{told}");
    let printed_lines: Vec<&str> = printed.lines().collect();
    assert_eq!(printed_lines[5], "renderer: processor", "{printed}");
}
