//! The one comparison that needs the installed reference, as a yardstick test: it starts the
//! reference from outside and reads what it says of itself. With the reference absent the runner
//! reports it as skipped, by name, and never as passed (`docs/dev/gates.md`, sections 5 and 11).
//!
//! It runs from the folder the tests run in, so the reference is looked for the way the comparison
//! tool looks for it: in the reference's folder nearest at or above it (`blackbox::default_folder`).

use parity::blackbox;

fn yardstick_reference_the_installed_reference_gives_a_version() {
    let folder = blackbox::default_folder(&std::env::current_dir().unwrap());
    let version = blackbox::version(&folder).unwrap_or_else(|why| panic!("the reference: {why}"));
    assert!(!version.is_empty(), "the reference gave an empty version");
    assert!(
        version.chars().any(|c| c.is_ascii_digit()),
        "the version has no digit in it: {version}"
    );
}

measure::yardstick_tests!(
    blackbox::needs();
    yardstick_reference_the_installed_reference_gives_a_version,
);
