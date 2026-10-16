//! The choice of the renderer by `renderer_for`, the one place where it is made
//! (`docs/spec/render-command.md` section 1.2). The card is replaced by a starter that the test
//! gives, so these tests run on a machine with no graphics card as well.

mod common;

use std::fs;

use cli::{renderer_for, Choice, Chosen, StartCard};
use common::project as make;
use draw::{DisplayList, Error, Picture, Renderer};
use draw_cpu::CpuRenderer;
use project::{Options, Project};

const GROUP: &str = "renderer-choice";

const FILM: &str = "\
<composition version=\"1\" id=\"film\" width=\"64\" height=\"48\" fps=\"10\" duration=\"0.5\" background=\"#204060\">
<box left=\"8\" top=\"8\" width=\"16\" height=\"16\" background=\"#ffffff\"/>
</composition>
";

const NO_CARD: &str = "this machine has no graphics card frx7 can draw with: no adapter was found";

/// A renderer that stands for the card: it draws as the processor does, and is named as the card.
struct FakeCard(CpuRenderer);

impl Renderer for FakeCard {
    fn name(&self) -> &'static str {
        "card"
    }

    fn draw(&mut self, list: &DisplayList) -> Result<Picture, Error> {
        self.0.draw(list)
    }
}

/// The starter of a card that starts.
fn card_starts() -> Result<Box<dyn Renderer>, Error> {
    Ok(Box::new(FakeCard(CpuRenderer)))
}

/// The starter of a machine with no card.
fn card_does_not_start() -> Result<Box<dyn Renderer>, Error> {
    Err(Error::new(NO_CARD))
}

/// The starter of a run that must not start the card at all.
fn card_must_not_start() -> Result<Box<dyn Renderer>, Error> {
    panic!("the card must not be started for this setting")
}

/// The project of `name`, with the project file `yaml` when one is given and the options `args`
/// (option name and value, in pairs) applied as the command applies them.
fn open(name: &str, yaml: Option<&str>, args: &[&str]) -> Project {
    let (folder, _cwd) = make(GROUP, name, FILM);
    if let Some(yaml) = yaml {
        fs::write(folder.join("framesmith.yaml"), yaml).unwrap();
    }
    let mut options = Options::default();
    for pair in args.chunks(2) {
        options.set(pair[0], pair[1]).unwrap_or_else(|error| panic!("{error}"));
    }
    let mut project = Project::open(&folder).unwrap_or_else(|error| panic!("the project does not open: {error}"));
    project.apply(&options);
    project
}

fn choose(project: &Project, start_card: &StartCard) -> Result<Choice, Error> {
    renderer_for(project.settings(), start_card)
}

fn chosen(result: Result<Choice, Error>) -> Choice {
    match result {
        Ok(choice) => choice,
        Err(error) => panic!("the renderer was refused: {error}"),
    }
}

#[test]
fn cpu_is_the_processor_and_never_starts_the_card() {
    let project = open("cpu", None, &["--backend", "cpu"]);
    let choice = chosen(choose(&project, &card_must_not_start));
    assert_eq!(choice.name(), "processor");
    assert_eq!(choice.chosen(), Chosen::Setting);
    assert_eq!(choice.summary_line(), "renderer: processor");
}

#[test]
fn gpu_is_the_card_when_the_card_starts() {
    let project = open("gpu-starts", None, &["--backend", "gpu"]);
    let choice = chosen(choose(&project, &card_starts));
    assert_eq!(choice.name(), "card");
    assert_eq!(choice.chosen(), Chosen::Setting);
    assert_eq!(choice.summary_line(), "renderer: card");
}

#[test]
fn gpu_is_refused_with_the_reason_of_the_card_and_nothing_falls_back_to_the_processor() {
    let project = open("gpu-refused", None, &["--backend", "gpu"]);
    match choose(&project, &card_does_not_start) {
        Ok(choice) => panic!("gpu gave the {} renderer on a machine with no card", choice.name()),
        Err(error) => assert_eq!(error.to_string(), NO_CARD),
    }
}

#[test]
fn a_gpu_setting_in_the_project_file_is_refused_on_a_machine_with_no_card() {
    let project = open("gpu-file-refused", Some("render:\n  backend: gpu\n"), &[]);
    assert!(choose(&project, &card_does_not_start).is_err());
}

#[test]
fn auto_takes_the_card_when_the_card_starts_and_says_so() {
    let project = open("auto-starts", None, &[]);
    let choice = chosen(choose(&project, &card_starts));
    assert_eq!(choice.name(), "card");
    assert_eq!(choice.chosen(), Chosen::AutoCard);
    assert_eq!(choice.summary_line(), "renderer: card (chosen by auto)");
}

#[test]
fn auto_takes_the_processor_when_no_card_starts_and_says_so() {
    let project = open("auto-no-card", None, &[]);
    let choice = chosen(choose(&project, &card_does_not_start));
    assert_eq!(choice.name(), "processor");
    assert_eq!(choice.chosen(), Chosen::AutoProcessor);
    assert_eq!(choice.summary_line(), "renderer: processor (chosen by auto: no suitable card)");
}

#[test]
fn auto_from_the_project_file_takes_the_processor_when_no_card_starts() {
    let project = open("auto-file-no-card", Some("render:\n  backend: auto\n"), &[]);
    let choice = chosen(choose(&project, &card_does_not_start));
    assert_eq!(choice.chosen(), Chosen::AutoProcessor);
}

#[test]
fn the_words_of_the_log_for_each_choice_are_the_ones_the_run_log_names() {
    assert_eq!(Chosen::Setting.as_str(), "setting");
    assert_eq!(Chosen::AutoCard.as_str(), "auto card");
    assert_eq!(Chosen::AutoProcessor.as_str(), "auto no card");
}
