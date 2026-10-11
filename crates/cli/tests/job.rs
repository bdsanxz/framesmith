//! T045: the render job of a real project, dumped to JSON and loaded back.

mod common;

use common::*;
use render::Job;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Renders the title-card fixture and returns the job the run created, as it was dumped.
fn dumped(test: &str, args: &[&str]) -> (PathBuf, String) {
    let project = fixture(test, "title-card");
    let file = project.parent().unwrap().join("job.json");
    let mut all = vec!["render", text(&project)];
    all.extend_from_slice(args);
    let ran = run_in(
        project.parent().unwrap(),
        &all,
        &[("FRAMESMITH_DEBUG_JOB", file.as_os_str() as &OsStr)],
    );
    assert_eq!(ran.code, 0, "{}", ran.all());
    (project, std::fs::read_to_string(&file).unwrap())
}

fn title_card() -> &'static (PathBuf, String) {
    static ONCE: std::sync::OnceLock<(PathBuf, String)> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| dumped("t045_title_card", &[]))
}

#[test]
fn t045_c1_the_loaded_job_equals_the_original_in_every_field() {
    let (project, json) = title_card();
    let job = Job::from_json(json).unwrap();
    // The job is the one of this project, with what the run worked out.
    let project = std::fs::canonicalize(project).unwrap();
    assert_eq!(job.project, project);
    assert_eq!(job.entry, "index.html");
    assert_eq!(job.output, project.join("out").join("title-card.mp4"));
    assert_eq!((job.fps, job.frames), (30, 120));
    assert_eq!(
        (job.manifest.name.as_str(), job.manifest.width, job.manifest.height),
        ("title-card", 1280, 720)
    );
    assert_eq!((job.manifest.clips.len(), job.manifest.tweens.len()), (3, 7));
    assert_eq!(
        (
            job.format.container.as_str(),
            job.format.codec.as_str(),
            job.format.pixel_format.as_str()
        ),
        ("mp4", "h264", "yuv420p")
    );
    assert_eq!(job.run_id.len(), 12, "{}", job.run_id);
    // The manifest in it is the one the check reads from the files.
    assert_eq!(Some(&job.manifest), model::check(&project).manifest.as_ref());
    // Field by field: the same value read twice is the same job, and every
    // field of the JSON is a field of the job.
    let original = Job::from_json(json).unwrap();
    assert_eq!(job, original);
    let written: serde_json::Value = serde_json::from_str(json).unwrap();
    let again: serde_json::Value = serde_json::from_str(&job.to_json()).unwrap();
    assert_eq!(written, again);
    let fields: Vec<&str> = written.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        fields,
        ["entry", "format", "fps", "frames", "manifest", "output", "project", "run_id"]
    );
}

#[test]
fn t045_c2_dumping_the_loaded_job_gives_the_same_json() {
    let (_, json) = title_card();
    let loaded = Job::from_json(json).unwrap();
    assert_eq!(&loaded.to_json(), json);
    assert_eq!(Job::from_json(&loaded.to_json()).unwrap().to_json(), *json);
}

/// The message for a job whose JSON was changed by `change`.
fn rejected(change: impl Fn(&mut serde_json::Value)) -> String {
    let mut value: serde_json::Value = serde_json::from_str(&title_card().1).unwrap();
    change(&mut value);
    Job::from_json(&serde_json::to_string_pretty(&value).unwrap()).expect_err("the job is rejected")
}

#[test]
fn t045_c3_a_missing_field_an_unknown_field_and_a_wrong_type_are_rejected_by_name() {
    let missing = rejected(|job| {
        job.as_object_mut().unwrap().remove("frames");
    });
    assert_eq!(missing, "the render job is not valid: missing field `frames`");
    let missing_inside = rejected(|job| {
        job["manifest"]["clips"][1].as_object_mut().unwrap().remove("start");
    });
    assert_eq!(
        missing_inside,
        "the render job is not valid at `manifest.clips[1]`: missing field `start`"
    );
    let unknown = rejected(|job| job["workers"] = serde_json::json!(4));
    assert!(
        unknown.starts_with("the render job is not valid at `workers`: unknown field `workers`"),
        "{unknown}"
    );
    let unknown_inside = rejected(|job| job["format"]["bitrate"] = serde_json::json!("2M"));
    assert!(
        unknown_inside.starts_with("the render job is not valid at `format.bitrate`: unknown field `bitrate`"),
        "{unknown_inside}"
    );
    let wrong = rejected(|job| job["fps"] = serde_json::json!("thirty"));
    assert_eq!(
        wrong,
        "the render job is not valid at `fps`: invalid type: string \"thirty\", expected u32"
    );
    let wrong_inside = rejected(|job| job["manifest"]["width"] = serde_json::json!(-1280));
    assert!(
        wrong_inside.starts_with("the render job is not valid at `manifest.width`: invalid value: integer `-1280`"),
        "{wrong_inside}"
    );
    // Text that is not JSON at all is rejected too.
    assert!(Job::from_json("{ \"run_id\": ")
        .unwrap_err()
        .starts_with("the render job is not valid"));
}

#[test]
fn t045_c4_a_frame_rate_and_an_output_path_that_are_not_the_defaults_survive() {
    let (project, json) = dumped("t045_c4", &["--fps", "24", "--output", "elsewhere/card at 24.mp4"]);
    let job = Job::from_json(&json).unwrap();
    let output = std::fs::canonicalize(project.parent().unwrap())
        .unwrap()
        .join("elsewhere/card at 24.mp4");
    assert_eq!((job.fps, job.frames), (24, 96));
    assert_eq!(job.output, output);
    // The composition still says 30; the run's rate is the one asked for.
    assert_eq!(job.manifest.fps, 30);
    assert_eq!(job.to_json(), json);
    assert_eq!(Job::from_json(&job.to_json()).unwrap(), job);
    assert!(Path::new(&output).is_file(), "the run wrote the video where the job says");
    assert_eq!(facts(&output).frames, 96);
}
