//! The local override for work on both repositories at once: `scripts/gates/frx7.sh override` and
//! `drop-override`. See `docs/dev/frx7.md`, section 4.

mod common;

use common::{Repo, REMOTE};
use std::path::PathBuf;

/// A folder that looks like frx7: a workspace with crates, some of which the Framesmith test
/// repository needs (`mp4`, which needs `base` and `format`) and some it does not.
fn frx7_folder(repo: &Repo) -> PathBuf {
    let folder = repo
        .dir
        .parent()
        .expect("a parent")
        .join(format!("{}-frx7", repo.dir.file_name().expect("a name").to_string_lossy()));
    let _ = std::fs::remove_dir_all(&folder);
    let crate_ = |name: &str, dependencies: &str, dev: &str, other: &str| {
        let manifest = format!(
            "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n\n[dependencies]\n{dependencies}\n[dev-dependencies]\n{dev}\n{other}"
        );
        let path = folder.join("crates").join(name);
        std::fs::create_dir_all(path.join("src")).expect("the folder is made");
        std::fs::write(path.join("Cargo.toml"), manifest).expect("the manifest is written");
        std::fs::write(path.join("src/lib.rs"), "").expect("the source is written");
    };
    std::fs::create_dir_all(&folder).expect("the folder is made");
    std::fs::write(
        folder.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/*\"]\nresolver = \"2\"\n",
    )
    .expect("the workspace is written");
    crate_("base", "", "", "");
    crate_("format", "base = { workspace = true }\n", "", "");
    crate_(
        "mp4",
        "base = { workspace = true }\nformat = { workspace = true }\n",
        "testkit = { workspace = true }\n",
        "",
    );
    crate_("testkit", "base = { workspace = true }\n", "", "");
    crate_(
        "gpu",
        "base = { workspace = true }\nwgpu = { workspace = true }\n",
        "",
        "[target.'cfg(target_os = \"macos\")'.dependencies]\nobjc2 = \"0.6\"\n",
    );
    folder
}

#[test]
fn the_override_patches_the_crates_the_build_takes_and_the_ones_they_need_with_paths_into_the_local_folder() {
    let repo = Repo::pinned("override-written");
    let folder = frx7_folder(&repo);
    let run = repo.script(&["override", folder.to_str().expect("a path")]);
    assert_eq!(run.code, 0, "{}", run.text);
    let config = repo.read(".cargo/config.toml");
    assert!(config.starts_with("# frx7 local override"), "{config}");
    assert!(config.contains(&format!("[patch.\"{REMOTE}\"]")), "{config}");
    for needed in ["base", "format", "mp4"] {
        assert!(
            config.contains(&format!("{needed} = {{ path = \"{}/crates/{needed}\" }}", folder.display())),
            "{needed} is patched with a path into the folder: {config}"
        );
    }
    for not_needed in ["testkit", "gpu"] {
        assert!(
            !config.contains(not_needed),
            "{not_needed} is not needed, and an unused patch only makes Cargo warn: {config}"
        );
    }
    assert!(run.text.contains("frx7 is built from"), "{}", run.text);
}

#[test]
fn the_override_is_not_a_committable_file_and_the_gate_still_passes_with_it_in_place() {
    let repo = Repo::pinned("override-ignored");
    repo.write(
        ".cargo/.gitignore",
        &std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.cargo/.gitignore"))
            .expect("the ignore file is read"),
    );
    repo.commit("Ignore the override");
    let folder = frx7_folder(&repo);
    assert_eq!(repo.script(&["override", folder.to_str().expect("a path")]).code, 0);
    assert_eq!(
        repo.git(&["status", "--porcelain"]),
        "",
        "git sees nothing to commit while the override stands"
    );
    let run = repo.script(&["check"]);
    assert_eq!(run.code, 0, "{}", run.text);
    assert!(
        run.text.contains("note: a local override of frx7 stands in .cargo/config.toml"),
        "{}",
        run.text
    );
}

#[test]
fn the_override_refuses_a_folder_that_is_not_frx7_and_a_config_it_did_not_write() {
    let repo = Repo::pinned("override-refused");
    let nowhere = repo.dir.join("not-frx7");
    std::fs::create_dir_all(&nowhere).expect("the folder is made");
    let run = repo.script(&["override", nowhere.to_str().expect("a path")]);
    assert_eq!(run.code, 2, "{}", run.text);
    assert!(run.text.contains("is not a folder of frx7"), "{}", run.text);
    assert!(!repo.dir.join(".cargo/config.toml").exists());

    let folder = frx7_folder(&repo);
    repo.write(".cargo/config.toml", "[build]\njobs = 2\n");
    let run = repo.script(&["override", folder.to_str().expect("a path")]);
    assert_eq!(run.code, 1, "{}", run.text);
    assert!(
        run.text.contains("was not written by this command; it is left alone"),
        "{}",
        run.text
    );
    assert_eq!(
        repo.read(".cargo/config.toml"),
        "[build]\njobs = 2\n",
        "a configuration of the maintainer's own is untouched"
    );
}

#[test]
fn dropping_the_override_removes_its_file_and_only_its_file() {
    let repo = Repo::pinned("override-dropped");
    let folder = frx7_folder(&repo);
    assert_eq!(repo.script(&["override", folder.to_str().expect("a path")]).code, 0);
    let run = repo.script(&["drop-override"]);
    assert_eq!(run.code, 0, "{}", run.text);
    assert!(!repo.dir.join(".cargo/config.toml").exists());
    assert!(run.text.contains("removed .cargo/config.toml"), "{}", run.text);
    assert!(repo.script(&["drop-override"]).text.contains("no local override stands"));

    repo.write(".cargo/config.toml", "[build]\njobs = 2\n");
    let run = repo.script(&["drop-override"]);
    assert_eq!(run.code, 1, "{}", run.text);
    assert!(repo.dir.join(".cargo/config.toml").exists());
}
