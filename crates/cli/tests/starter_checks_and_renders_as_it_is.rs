//! The fresh starter passes `framesmith check` with exit 0, and `framesmith render` completes with exit 0, as it is
//! (`docs/spec/init-command.md`, section 8).

mod common;

use common::{err, framesmith, out, scratch};

#[test]
fn the_fresh_starter_passes_check_and_renders_with_exit_0() {
    let cwd = scratch("init", "check-and-render");
    let init = framesmith(&cwd, ["init", "demo"], &[]);
    assert_eq!(init.status.code(), Some(0), "{}{}", out(&init), err(&init));

    let check = framesmith(&cwd, ["check", "demo"], &[]);
    assert_eq!(check.status.code(), Some(0), "check: {}{}", out(&check), err(&check));
    assert_eq!(err(&check), "", "check reports nothing on standard error");

    let render = framesmith(&cwd, ["render", "demo"], &[]);
    assert_eq!(render.status.code(), Some(0), "render: {}{}", out(&render), err(&render));
    assert!(
        cwd.join("demo/renders/demo.mp4").is_file(),
        "the film is written where the project file says"
    );
}
