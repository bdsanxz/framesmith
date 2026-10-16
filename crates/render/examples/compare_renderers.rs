//! Compares the card's pictures with the processor's for every frame of a project, and prints the
//! figures of each frame (`docs/spec/card-renderer.md` section 6). A development tool: it draws
//! pictures and writes no file. `--doctor <frame>` raises the red of the card's picture of that frame,
//! to see the failure the comparison gives (section 7).
//!
//! Usage: `cargo run -p render --example compare_renderers -- <project folder> [--doctor <frame>]`.
//! Exit status: 0 when every frame is within the tolerance; 1 when a frame is not, or the card cannot
//! be started; 2 for an argument that is not understood, or a project that is not drawn.

#[path = "../tests/common/agreement.rs"]
mod agreement;

use std::path::PathBuf;
use std::process::ExitCode;

use agreement::compare_scene;
use draw_cpu::CpuRenderer;
use draw_gpu::CardRenderer;
use project::Project;
use render::Scene;

const USAGE: &str = "usage: compare_renderers <project folder> [--doctor <frame>]";

/// The folder of the project, and the frame the doctored card changes when one is asked for.
fn arguments() -> Result<(PathBuf, Option<u64>), String> {
    let mut folder = None;
    let mut doctored = None;
    let mut words = std::env::args().skip(1);
    while let Some(word) = words.next() {
        if word == "--doctor" {
            let value = words.next().ok_or("`--doctor` needs a frame number")?;
            let number = value.parse::<u64>().map_err(|_| format!("`{value}` is not a frame number"))?;
            doctored = Some(number);
        } else if folder.is_none() && !word.starts_with('-') {
            folder = Some(PathBuf::from(word));
        } else {
            return Err(format!("`{word}` is not understood"));
        }
    }
    let folder = folder.ok_or("a project folder is needed")?;
    Ok((folder, doctored))
}

fn main() -> ExitCode {
    let (folder, doctored) = match arguments() {
        Ok(arguments) => arguments,
        Err(message) => {
            eprintln!("{message}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let project = match Project::open(&folder) {
        Ok(project) => project,
        Err(error) => {
            eprintln!("the project did not open: {error:?}");
            return ExitCode::from(2);
        }
    };
    let scene = match Scene::from_project(&project) {
        Ok(scene) => scene,
        Err(reports) => {
            eprintln!("the project is not drawn: {reports:?}");
            return ExitCode::from(2);
        }
    };
    if let Some(number) = doctored {
        if number >= scene.frames() {
            eprintln!("frame {number} is not a frame of the project, which has {}", scene.frames());
            return ExitCode::from(2);
        }
    }
    let mut card = match CardRenderer::new() {
        Ok(card) => card,
        Err(error) => {
            eprintln!("the card could not be started: {error}");
            return ExitCode::from(1);
        }
    };
    let comparison = compare_scene(&scene, &mut CpuRenderer, &mut card, doctored);
    for line in comparison.report() {
        println!("{line}");
    }
    if comparison.passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
