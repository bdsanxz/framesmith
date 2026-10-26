//! The output of a film is staged beside its path and moved in only when it is whole: an earlier file at the
//! output path keeps its bytes during the render and after a render that fails, a completed render leaves no
//! staging file, and a folder that is missing is made (`docs/spec/render-command.md`, sections 5 and 6).

mod common;

use common::{project_from_fixture, scratch};
use draw::{DisplayList, Error, Picture, Renderer};
use draw_cpu::CpuRenderer;
use measure::video_facts;
use project::Project;
use render::{check_place, remove_stale_staging, staging_folder_of, write_film, write_film_stepped, Encoding, Scene, Step};
use std::fs;
use std::path::{Path, PathBuf};

const EARLIER: &[u8] = b"an earlier film, kept until the new one is whole";

fn scene_of(name: &str) -> Scene {
    let folder = project_from_fixture("staged-output", name, "first-render");
    Scene::from_project(&Project::open(&folder).unwrap(), &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

/// The names of the files in a folder, in order.
fn names_in(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

/// Draws with the processor, and before each frame looks at the output path and at its staging file.
/// The frame whose number is `fail_at` is refused, so that a render can be stopped in its middle.
struct Probe {
    inner: CpuRenderer,
    output: PathBuf,
    calls: u64,
    fail_at: Option<u64>,
    bytes_before_each_frame: Vec<Option<Vec<u8>>>,
    staging_before_each_frame: Vec<bool>,
}

impl Probe {
    fn new(output: &Path, fail_at: Option<u64>) -> Probe {
        Probe {
            inner: CpuRenderer,
            output: output.to_path_buf(),
            calls: 0,
            fail_at,
            bytes_before_each_frame: Vec::new(),
            staging_before_each_frame: Vec::new(),
        }
    }
}

impl Renderer for Probe {
    fn name(&self) -> &'static str {
        "processor"
    }

    fn draw(&mut self, list: &DisplayList) -> Result<Picture, Error> {
        self.bytes_before_each_frame.push(fs::read(&self.output).ok());
        self.staging_before_each_frame
            .push(staging_folder_of(&self.output).join("film.part.mp4").is_file());
        let number = self.calls;
        self.calls += 1;
        if self.fail_at == Some(number) {
            return Err(Error::new("the drawing was stopped on purpose"));
        }
        self.inner.draw(list)
    }

    fn draws_text(&self) -> bool {
        true
    }
}

#[test]
fn a_sweep_at_each_step_of_a_start_never_removes_the_folder_of_the_render_that_starts() {
    let folder = scratch("staged-output", "sweep-during-start");
    let output = folder.join("film.mp4");
    let scene = scene_of("first-render");
    let mut renderer = Probe::new(&output, None);
    let mut seen: Vec<(Step, Vec<String>)> = Vec::new();
    let written = write_film_stepped(&scene, Vec::new(), &mut renderer, &output, 128, Encoding::default(), None, &mut |step| {
        // A second render into the same folder sweeps while the first one stands at this step.
        remove_stale_staging(&folder);
        let hidden = names_in(&folder)
            .into_iter()
            .filter(|name| name.starts_with(".framesmith-"))
            .collect();
        seen.push((step, hidden));
    })
    .unwrap_or_else(|failure| panic!("{failure}"));

    let own = staging_folder_of(&output).file_name().unwrap().to_str().unwrap().to_string();
    let own_lock = format!("{own}.lock");
    assert_eq!(
        seen.iter().map(|(step, _)| *step).collect::<Vec<_>>(),
        vec![Step::Locked, Step::Made],
        "the steps of a start are not the ones the render takes"
    );
    assert_eq!(
        seen[0].1,
        vec![own_lock.clone()],
        "while starting, before its folder is made, the run's lock file is the only hidden name"
    );
    assert_eq!(
        seen[1].1,
        vec![own.clone(), own_lock.clone()],
        "the sweep of another render removed the lock or the folder of the render that starts"
    );
    assert_eq!(written.frames, scene.frames());
    assert!(output.is_file(), "the film is not at its output path");
}

#[test]
fn an_earlier_film_keeps_its_bytes_during_the_render_and_a_staging_file_holds_the_new_one() {
    let scene = scene_of("during");
    let folder = scratch("staged-output", "during-folder");
    let output = folder.join("film.mp4");
    fs::write(&output, EARLIER).unwrap();

    let mut probe = Probe::new(&output, None);
    write_film(&scene, &mut probe, &output, 192).unwrap();

    assert_eq!(probe.bytes_before_each_frame.len(), 10, "one look before each of the ten frames");
    for (number, bytes) in probe.bytes_before_each_frame.iter().enumerate() {
        assert_eq!(bytes.as_deref(), Some(EARLIER), "the output path during frame {number}");
    }
    assert!(
        probe.staging_before_each_frame.iter().all(|present| *present),
        "the staging file is there before every frame: {:?}",
        probe.staging_before_each_frame
    );
    let facts = video_facts(&output).unwrap();
    assert_eq!((facts.codec.as_str(), facts.frames), ("h264", 10));
}

#[test]
fn a_completed_render_leaves_the_film_alone_in_its_folder_with_no_staging_file() {
    let scene = scene_of("completed");
    let folder = scratch("staged-output", "completed-folder");
    let output = folder.join("film.mp4");
    fs::write(&output, EARLIER).unwrap();

    write_film(&scene, &mut CpuRenderer, &output, 192).unwrap();

    assert_eq!(names_in(&folder), ["film.mp4"]);
    assert_ne!(fs::read(&output).unwrap(), EARLIER, "the new film replaced the earlier one");
    assert_eq!(video_facts(&output).unwrap().frames, 10);
}

#[test]
fn a_render_that_fails_keeps_the_earlier_film_and_leaves_no_staging_file() {
    let scene = scene_of("failed");
    let folder = scratch("staged-output", "failed-folder");
    let output = folder.join("film.mp4");
    fs::write(&output, EARLIER).unwrap();

    let mut probe = Probe::new(&output, Some(4));
    let failed = write_film(&scene, &mut probe, &output, 192);

    assert!(failed.is_err(), "the render was stopped on frame 4");
    assert_eq!(fs::read(&output).unwrap(), EARLIER, "the earlier film keeps its bytes");
    assert_eq!(names_in(&folder), ["film.mp4"], "no staging file is left");
}

#[test]
fn a_render_to_a_folder_that_is_missing_makes_it_and_writes_the_film() {
    let scene = scene_of("missing");
    let base = scratch("staged-output", "missing-folder");
    let output = base.join("made").join("deeper").join("film.mp4");

    write_film(&scene, &mut CpuRenderer, &output, 192).unwrap();

    assert_eq!(names_in(&base.join("made").join("deeper")), ["film.mp4"]);
    assert_eq!(video_facts(&output).unwrap().frames, 10);
}

#[test]
fn a_place_that_can_be_written_is_checked_without_leaving_a_file() {
    let folder = scratch("staged-output", "place-open");
    let output = folder.join("film.mp4");

    check_place(&output).unwrap();

    assert!(names_in(&folder).is_empty(), "the check left {:?}", names_in(&folder));
}

#[test]
fn a_folder_that_cannot_be_written_is_refused_with_the_path_and_the_systems_reason_and_nothing_is_written() {
    let folder = scratch("staged-output", "place-locked");
    let locked = folder.join("locked");
    fs::create_dir_all(&locked).unwrap();
    fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o555)).unwrap();
    let output = locked.join("film.mp4");

    let refusal = check_place(&output);
    fs::set_permissions(&locked, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();

    let refusal = refusal.expect_err("a folder without write permission is refused");
    assert_eq!(refusal.path(), output.as_path());
    assert!(refusal.reason().contains("Permission denied"), "{}", refusal.reason());
    assert!(names_in(&locked).is_empty(), "the refused place holds {:?}", names_in(&locked));
}

#[test]
fn a_move_that_fails_at_the_end_leaves_no_staging_file_and_the_folder_at_the_output_path_stays() {
    let scene = scene_of("move-failed");
    let folder = scratch("staged-output", "move-failed-folder");
    let output = folder.join("film.mp4");
    fs::create_dir_all(&output).unwrap();

    let failed = write_film(&scene, &mut CpuRenderer, &output, 192);

    assert!(failed.is_err(), "a folder at the output path cannot be replaced by the film");
    assert_eq!(names_in(&folder), ["film.mp4"], "no staging file is left");
    assert!(output.is_dir(), "the folder at the output path is still there");
}
