# Framesmith

Renders a composition (a folder of plain files: XML in our own notation, media, fonts, and an optional `framesmith.yaml`) to video on the user's own machine, with nothing else to install. Its media work is done by frx7, our own media stack, in a repository of its own.

macOS only for now. One cargo workspace; every part is a crate under `crates/`, named by its role with no prefix. The behaviour of each is specified in `docs/spec/`, in our own words, before the code.

## Build and test

```
cargo build --workspace
cargo test --workspace
```

## The quality gates

Four gates stand between a change and this repository: formatting, static analysis, the tests and the licence gate. Their behaviour is specified in `docs/dev/gates.md`.

```
scripts/gates/install-hooks.sh    # once after cloning: a failing gate then stops every push
scripts/gates.sh                  # the four gates, then the tests marked "reference machine only" and the yardstick tests
```

`cargo-deny` is needed for the licence gate: `cargo install --locked cargo-deny --version 0.20.2`. No browser and no FFmpeg is needed by any gate.

`scripts/gates.sh --gate lint`, `-p CRATE` and the like run a part and say they are narrowed; a narrowed run exits 3 at best and is never a pass of the gates. The hosted run (`.github/workflows/gates.yml`) runs on `main` and on pull requests only and reports the four gates each by name. Work branches rely on the local gates.

### Tests that need the hardware encoder or a graphics card

A test that needs the hardware encoder or a card cannot be judged on a hosted runner. Mark it in one way: its name starts with `reference_machine_only_` and it carries `#[ignore = "reference machine only: <the reason>"]`. The hosted run lists it as not run, by name, and never as passed; `scripts/gates.sh` runs it before every push.

### Tests that compare with a program from outside

A test that starts a program the developer installed (today `ffmpeg` and `ffprobe`, to make and read test videos) is a yardstick test. Mark it in one way: its name starts with `yardstick_` and it carries `#[ignore = "yardstick: <the reason>"]`. The hosted run, and a machine without the programs, list it as not run, by name, and never as passed; `scripts/gates.sh` runs it on a machine that has them.

### Tests stand alone

A test never reads a file that another test writes and never relies on the order tests run in. It writes the files it reads, under names of its own.

## Dependencies

Each dependency is recorded in `PROGRESS.md` of the workspace with its name, licence and reason. `scripts/gates/dependencies.sh list` prints the outside crates of `Cargo.lock` with their licences; `scripts/gates/dependencies.sh compare FILE` shows which ones a record leaves out. The licence gate refuses GPL, AGPL, LGPL, SSPL and unknown licences.
