//! Writes both faces of the test font into the folder it is given:
//!
//!     cargo run -p measure --example write_test_font -- fixtures/fonts
//!
//! The files checked in under `fixtures/fonts/` are written this way (`docs/spec/test-font.md`,
//! section 6). Nothing else reads this program.

use measure::font::Face;
use std::path::PathBuf;

fn main() {
    let Some(folder) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: write_test_font <folder>");
        std::process::exit(2);
    };
    if let Err(error) = std::fs::create_dir_all(&folder) {
        eprintln!("write_test_font: {}: {error}", folder.display());
        std::process::exit(1);
    }
    for face in Face::ALL {
        let path = folder.join(face.file_name());
        let bytes = face.bytes();
        if let Err(error) = std::fs::write(&path, &bytes) {
            eprintln!("write_test_font: {}: {error}", path.display());
            std::process::exit(1);
        }
        println!("{} {} bytes", path.display(), bytes.len());
    }
}
