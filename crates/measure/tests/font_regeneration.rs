//! The checked-in files of the test font are the output of its generator, byte for byte, and the
//! generator gives the same bytes on every run (`docs/spec/test-font.md`, section 6).

use measure::font::Face;
use std::path::PathBuf;

fn checked_in_path(face: Face) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/fonts")
        .join(face.file_name())
}

#[test]
fn regenerated_faces_equal_the_checked_in_files() {
    for face in Face::ALL {
        let path = checked_in_path(face);
        let checked_in = std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let regenerated = face.bytes();
        println!(
            "{}: checked in {} bytes, regenerated {} bytes",
            path.display(),
            checked_in.len(),
            regenerated.len()
        );
        assert!(regenerated == checked_in, "{} differs from the generator's output", path.display());
    }
}

#[test]
fn regenerating_a_face_twice_gives_the_same_bytes() {
    for face in Face::ALL {
        assert_eq!(face.bytes(), face.bytes(), "{:?} is not deterministic", face);
    }
}

#[test]
fn each_face_file_is_under_one_hundred_kilobytes() {
    for face in Face::ALL {
        let length = std::fs::metadata(checked_in_path(face)).expect("the file is there").len();
        assert!(length < 100 * 1024, "{} is {length} bytes", face.file_name());
    }
}
