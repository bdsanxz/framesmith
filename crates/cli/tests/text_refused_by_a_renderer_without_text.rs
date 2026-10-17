//! `framesmith render` with a renderer that draws no text, run in the process with the starter of that renderer given
//! by the test, so that it runs on every machine with no graphics card (`docs/spec/text.md`, section 9;
//! `docs/spec/render-command.md`, sections 1.2 and 7). The first `<text>` of the film is refused with its line,
//! before any frame, and writes no film; a film with no text is rendered. The card itself is tested on the reference
//! machine, which has one.

mod common;

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use common::project as make;
use draw::{DisplayList, Error, Picture, Renderer};
use draw_cpu::CpuRenderer;

const GROUP: &str = "text-refused-by-a-renderer-without-text";

const WITH_TEXT: &str = "\
<composition version=\"1\" id=\"film\" width=\"96\" height=\"54\" fps=\"10\" duration=\"0.5\" background=\"#ffffff\">
<font src=\"fonts/framesmith-test-stroke-400.ttf\" family=\"Framesmith Test Stroke\"/>
<text left=\"10\" top=\"10\" font-family=\"Framesmith Test Stroke\" font-size=\"20\">AB</text>
</composition>
";

const WITHOUT_TEXT: &str = "\
<composition version=\"1\" id=\"film\" width=\"96\" height=\"54\" fps=\"10\" duration=\"0.5\" background=\"#204060\">
<box left=\"8\" top=\"8\" width=\"16\" height=\"16\" background=\"#ffffff\"/>
</composition>
";

/// `run_with` changes the working folder, which is one for the whole process: the tests take turns.
static TURNS: Mutex<()> = Mutex::new(());

/// A renderer that stands for the card: it draws a picture as the processor does, and, as the card does in this
/// version, it draws no text (the default of `Renderer::draws_text`).
struct NoTextRenderer(CpuRenderer);

impl Renderer for NoTextRenderer {
    fn name(&self) -> &'static str {
        "card"
    }

    fn draw(&mut self, list: &DisplayList) -> Result<Picture, Error> {
        self.0.draw(list)
    }
}

fn card_starts() -> Result<Box<dyn Renderer>, Error> {
    Ok(Box::new(NoTextRenderer(CpuRenderer)))
}

/// A project of `composition`, whose folder holds the test font that the composition declares.
fn project_with(name: &str, composition: &str) -> (PathBuf, PathBuf) {
    let (project, cwd) = make(GROUP, name, composition);
    fs::create_dir_all(project.join("fonts")).unwrap();
    let file = "framesmith-test-stroke-400.ttf";
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fonts").join(file);
    fs::copy(&from, project.join("fonts").join(file)).unwrap();
    (project, cwd)
}

/// Runs `framesmith render <project> --backend gpu` in the process, with the card's starter given, and returns the
/// exit code, what was printed, and what was printed to the error output.
fn render_on_the_card(cwd: &Path, project: &Path) -> (u8, String, String) {
    let _turn = TURNS.lock().unwrap_or_else(|poison| poison.into_inner());
    std::env::set_current_dir(cwd).unwrap();
    let args: Vec<OsString> = vec!["render".into(), project.as_os_str().to_owned(), "--backend".into(), "gpu".into()];
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let code = cli::run_with(&args, &mut out, &mut err, &card_starts);
    (code, String::from_utf8(out).unwrap(), String::from_utf8(err).unwrap())
}

#[test]
fn a_text_on_a_renderer_that_draws_no_text_is_refused_with_its_line_and_writes_no_film() {
    let (project, cwd) = project_with("text-refused", WITH_TEXT);
    let (code, printed, told) = render_on_the_card(&cwd, &project);

    assert_eq!(code, 2, "{printed}{told}");
    assert_eq!(told, "", "{printed}{told}");
    assert!(
        printed.contains("line 3: FS-0035")
            && printed.contains(
                "<text> is not drawn by the card renderer in this version, which draws no text; \
                 the processor draws it: use `--backend cpu`"
            ),
        "{printed}"
    );
    assert!(!printed.contains("Rendered "), "{printed}");
    assert!(!project.join("renders").exists(), "a folder was made for a film that was refused");
}

#[test]
fn a_film_with_no_text_is_rendered_by_a_renderer_that_draws_no_text() {
    let (project, cwd) = project_with("no-text", WITHOUT_TEXT);
    let (code, printed, told) = render_on_the_card(&cwd, &project);

    assert_eq!(code, 0, "{printed}{told}");
    assert_eq!(told, "", "{printed}{told}");
    assert!(printed.contains("renderer: card"), "{printed}");
}
