# Framesmith

Renders a composition (a folder of plain files: XML in our own notation, media, fonts, and an optional `framesmith.yaml`) to video on the user's own machine, with nothing else to install. Its media work is done by frx7, our own media stack, in a repository of its own.

macOS only for now. One cargo workspace; every part is a crate under `crates/`, named by its role with no prefix. The behaviour of each is specified in `docs/spec/`, in our own words, before the code.

## Build and test

```
cargo build --workspace
cargo test --workspace
```

frx7, the media stack, is a repository of its own with a private remote. This repository takes it at one pinned revision (`Cargo.toml`, `docs/dev/frx7.md`), so a fresh copy builds on a machine with no frx7 folder: Cargo fetches the revision with the git credentials you already have for `github.com` (for example `gh auth setup-git`). To work on both at once, build against your local frx7 folder with an override that is never committed:

```
scripts/gates/frx7.sh override /path/to/frx7    # writes .cargo/config.toml, which git ignores
scripts/gates/frx7.sh drop-override             # back to the pin
```

## The quality gates

Six gates stand between a change and this repository: formatting, static analysis, the tests, the core suite without FFmpeg (the tests again with only the system's folders on the search path), the licence gate and the frx7 pin (no committed path override, one pinned revision, a moved pin carries its record). Their behaviour is specified in `docs/dev/gates.md` and `docs/dev/frx7.md`.

```
scripts/gates/install-hooks.sh    # once after cloning: a failing gate then stops every push
scripts/gates.sh                  # the six gates, then the tests marked "reference machine only" and the yardstick tests
```

`cargo-deny` is needed for the licence gate: `cargo install --locked cargo-deny --version 0.20.2`. No browser and no FFmpeg is needed by any gate.

`scripts/gates.sh --gate lint`, `-p CRATE` and the like run a part and say they are narrowed; a narrowed run exits 3 at best and is never a pass of the gates. The hosted run (`.github/workflows/gates.yml`) runs on `main` and on pull requests only and reports the six gates each by name; its jobs that build fetch frx7 with a read-only deploy key kept in the Actions secret `FRX7_DEPLOY_KEY`. Work branches rely on the local gates.

The hosted run `.github/workflows/other-systems.yml` builds and tests, on a Linux and a Windows runner, exactly the crates that touch no system interface, as recorded in `scripts/gates/system-free.txt` (`scripts/gates/system-free.sh list` works them out, `check` holds the record to the workspace, `test` runs them). A crate added later that touches no system interface fails both jobs, by name, until it is in the record. These jobs build nothing for a user: Framesmith is offered for macOS only.

### Tests that need the hardware encoder or a graphics card

A test that needs the hardware encoder or a card cannot be judged on a hosted runner. Mark it in one way: its name starts with `reference_machine_only_` and it carries `#[ignore = "reference machine only: <the reason>"]`. The hosted run lists it as not run, by name, and never as passed; `scripts/gates.sh` runs it before every push.

### Tests that compare with a program from outside

A test that starts a program the developer installed to compare (today `ffmpeg` and `ffprobe`, to make and read test videos, and the reference) is a yardstick test. Mark it in one way: its name starts with `yardstick_` (`yardstick_reference_` for one that needs the reference), it sits in a test file or folder whose name starts with `yardstick`, and its test target has `harness = false` with `measure::yardstick_tests!` as its `main`. A plain `cargo test` lists it as ignored; `cargo test --workspace -- --ignored yardstick_` runs it, and where its program is not installed it is reported as skipped, by name, and never as passed. The hosted run lists it as not run. A skipped comparison is never evidence that a line of the roadmap passes.

`scripts/gates.sh --gate core` (part of every full run and a job of the hosted run) runs the core suite, every test but the yardstick tests, with only `/usr/bin:/bin:/usr/sbin:/sbin` on the search path, under a system sandbox that refuses to start `ffmpeg`, `ffprobe` or the reference, and fails with the name of the test that tries. Details: `docs/dev/gates.md`, sections 5, 10 and 11.

### Tests stand alone

A test never reads a file that another test writes and never relies on the order tests run in. It writes the files it reads, under names of its own.

## Dependencies

Each dependency is recorded in `PROGRESS.md` of the workspace with its name, licence and reason. `scripts/gates/dependencies.sh list` prints the outside crates of `Cargo.lock` with their licences; `scripts/gates/dependencies.sh frx7` lists the crates of frx7 apart, with the revision; `scripts/gates/dependencies.sh compare FILE` shows which ones a record leaves out. The licence gate refuses GPL, AGPL, LGPL, SSPL and unknown licences.
