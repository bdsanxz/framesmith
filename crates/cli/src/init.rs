//! `framesmith init`: writes a starter project into a new folder.

use crate::text;
use render::Failure;
use std::path::Path;

/// The starter project. Every file is written by us.
const STARTER: [(&str, &str); 2] = [
    ("index.html", include_str!("../starter/index.html")),
    ("style.css", include_str!("../starter/style.css")),
];

fn create(folder: &Path) -> Result<(), Failure> {
    let cannot = |reason: String| {
        Failure::new(
            "FS-R015",
            format!("The folder {} cannot be created: {reason}.", folder.display()),
            "Choose a folder in a place you can write to.",
        )
    };
    match std::fs::read_dir(folder) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                return Err(Failure::new(
                    "FS-R014",
                    format!(
                        "The folder {} already exists and is not empty; nothing was written.",
                        folder.display()
                    ),
                    "Choose a folder name that does not exist yet, or empty the folder first.",
                ));
            }
        }
        Err(_) if folder.exists() => return Err(cannot("something that is not a folder is already there".to_string())),
        Err(_) => std::fs::create_dir_all(folder).map_err(|error| cannot(error.to_string()))?,
    }
    for (name, content) in STARTER {
        std::fs::write(folder.join(name), content).map_err(|error| cannot(format!("{name} could not be written ({error})")))?;
    }
    Ok(())
}

pub fn run(folder: &Path) -> i32 {
    tracing::info!(target: "cli", command = "init", folder = %folder.display(), "command started");
    match create(folder) {
        Ok(()) => {
            let shown = folder.display();
            println!("Created {shown}");
            println!("Next:");
            println!("  framesmith check {shown}");
            println!("  framesmith render {shown}");
            tracing::info!(target: "cli", command = "init", exit = 0, "command finished");
            0
        }
        Err(failure) => {
            eprint!("{}", text::failure_block(&failure));
            tracing::error!(target: "cli", command = "init", exit = 1, code = %failure.code, "command finished");
            1
        }
    }
}
