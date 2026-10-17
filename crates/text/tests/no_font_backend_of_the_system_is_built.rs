//! No font of the machine can be found, because no font backend of a system is built in
//! (`docs/spec/text.md` section 10). The font library reads the system's fonts only through its
//! `system` feature, which brings in the font interface of each system. The lock file lists the
//! dependencies of every system whatever the machine it is made on, so the backends are looked for
//! there: none is in the build, and the font library depends on none.

use std::path::Path;

/// The packages that the font library's `system` feature brings in: the font interfaces of macOS
/// and iOS, Linux, Windows and Android, and what they stand on.
const BACKENDS: [&str; 8] = [
    "objc2-core-text",
    "objc2-core-foundation",
    "objc2-foundation",
    "objc2",
    "yeslogic-fontconfig-sys",
    "windows",
    "windows-core",
    "roxmltree",
];

/// The names in the `dependencies` list of the package `name` in the lock file `lock`, each
/// without the version that may follow it.
fn dependencies_of(lock: &str, name: &str) -> Vec<String> {
    let start = lock
        .split("[[package]]")
        .find(|entry| entry.lines().any(|line| line.trim() == format!("name = \"{name}\"")))
        .unwrap_or_else(|| panic!("{name} is not in the lock file"));
    let Some(list) = start.split("dependencies = [").nth(1) else {
        return Vec::new();
    };
    list.split(']')
        .next()
        .unwrap_or("")
        .lines()
        .map(|line| line.trim().trim_end_matches(',').trim_matches('"'))
        .filter(|line| !line.is_empty())
        .map(|line| line.split(' ').next().unwrap_or(line).to_string())
        .collect()
}

fn lock() -> String {
    let place = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock");
    std::fs::read_to_string(&place).unwrap_or_else(|error| panic!("{}: {error}", place.display()))
}

#[test]
fn the_font_library_depends_on_no_font_backend_of_a_system() {
    let found = dependencies_of(&lock(), "fontique");
    assert!(!found.is_empty(), "the font library's dependencies were not found in the lock file");
    let backends: Vec<&String> = found.iter().filter(|name| BACKENDS.contains(&name.as_str())).collect();
    assert!(
        backends.is_empty(),
        "the font library depends on {backends:?}, a font backend of a system"
    );
}

#[test]
fn no_font_interface_of_a_system_is_in_the_build() {
    let lock = lock();
    for name in ["objc2-core-text", "yeslogic-fontconfig-sys"] {
        let line = format!("name = \"{name}\"");
        assert!(!lock.lines().any(|held| held.trim() == line), "{name} is in the lock file");
    }
}

#[test]
fn a_font_library_built_with_its_system_feature_is_found_out() {
    let with_backend = "[[package]]\nname = \"fontique\"\nversion = \"0.12.0\"\ndependencies = [\n \"hashbrown 0.17.1\",\n \"objc2-core-text\",\n \"read-fonts\",\n]\n";
    let found = dependencies_of(with_backend, "fontique");
    assert_eq!(found, ["hashbrown", "objc2-core-text", "read-fonts"]);
    assert!(found.iter().any(|name| BACKENDS.contains(&name.as_str())));
}
