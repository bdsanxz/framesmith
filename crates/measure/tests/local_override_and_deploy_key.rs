//! The two commands around the pin that are not a gate: the local override for work on both
//! repositories at once, and the hosted runner's deploy key. See `docs/dev/frx7.md`, sections 2
//! and 4.

mod common;

use common::{Repo, REMOTE};
use std::os::unix::fs::PermissionsExt;
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

/// A throwaway SSH key pair made by `ssh-keygen`, in the repository's folder.
fn throwaway_key(repo: &Repo) -> String {
    let key = repo.dir.join("throwaway_key");
    let made = common::isolate_from_runner(&mut std::process::Command::new("ssh-keygen"))
        .args(["-q", "-t", "ed25519", "-N", "", "-C", "throwaway", "-f"])
        .arg(&key)
        .status()
        .expect("ssh-keygen is run");
    assert!(made.success());
    std::fs::read_to_string(&key).expect("the key is read")
}

#[test]
fn the_deploy_key_is_refused_without_the_secret_and_without_a_real_key() {
    let repo = Repo::pinned("deploy-key-refused");
    let home = repo.dir.join("home");
    std::fs::create_dir_all(&home).expect("the folder is made");
    let home_text = home.to_str().expect("a path");

    let run = repo.script_with(&["deploy-key"], &[("HOME", home_text), ("FRX7_DEPLOY_KEY", "")]);
    assert_eq!(run.code, 1, "{}", run.text);
    assert!(
        run.text.contains("the variable FRX7_DEPLOY_KEY is empty or not set"),
        "{}",
        run.text
    );
    assert!(run.text.contains(REMOTE), "the refusal names the remote: {}", run.text);

    let run = repo.script_with(
        &["deploy-key"],
        &[("HOME", home_text), ("FRX7_DEPLOY_KEY", "this is a secret that is no key")],
    );
    assert_eq!(run.code, 1, "{}", run.text);
    assert!(run.text.contains("is not a private SSH key ssh can read"), "{}", run.text);
    assert!(
        !run.text.contains("this is a secret"),
        "the content of the variable is never shown: {}",
        run.text
    );
    assert!(
        !home.join(".ssh/frx7_deploy_key").exists(),
        "a key that cannot be read is not left behind"
    );
}

#[test]
fn the_deploy_key_is_installed_for_the_remote_and_never_printed() {
    let repo = Repo::pinned("deploy-key-installed");
    let secret = throwaway_key(&repo);
    let home = repo.dir.join("home");
    std::fs::create_dir_all(&home).expect("the folder is made");
    let git_config_path = home.join(".gitconfig");
    let run = repo.script_with(
        &["deploy-key"],
        &[
            ("HOME", home.to_str().expect("a path")),
            ("GIT_CONFIG_GLOBAL", git_config_path.to_str().expect("a path")),
            ("FRX7_DEPLOY_KEY", &secret),
        ],
    );
    assert_eq!(run.code, 0, "{}", run.text);
    for line in secret.lines().filter(|line| line.len() > 20) {
        assert!(!run.text.contains(line), "a line of the key was printed: {}", run.text);
    }
    let key = home.join(".ssh/frx7_deploy_key");
    assert_eq!(std::fs::read_to_string(&key).expect("the key is read").trim(), secret.trim());
    assert_eq!(
        std::fs::metadata(&key).expect("the key is there").permissions().mode() & 0o777,
        0o600,
        "only its owner reads the key"
    );

    let config = std::fs::read_to_string(home.join(".ssh/config")).expect("the ssh configuration is read");
    assert!(
        config.contains("Host frx7-github") && config.contains("HostName github.com") && config.contains("IdentitiesOnly yes"),
        "{config}"
    );
    assert!(config.contains("StrictHostKeyChecking yes"), "{config}");
    let hosts = std::fs::read_to_string(home.join(".ssh/frx7_known_hosts")).expect("the host keys are read");
    assert!(hosts.starts_with("github.com ssh-ed25519 AAAA"), "{hosts}");

    let git_config = std::fs::read_to_string(&git_config_path).expect("the git configuration is read");
    assert!(
        git_config.contains("git@frx7-github:bdsanxz/frx7") && git_config.contains(&format!("insteadOf = {REMOTE}")),
        "{git_config}"
    );
}
