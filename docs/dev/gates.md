# The quality gates of the Framesmith repository

Status: in force from Sprint 1. Sources: `CLAUDE.md` rule 11 (licences, and where hosted jobs run); decisions A7 (hosted jobs on `main` and on pull requests only) and A27 (tests that need the hardware encoder or a card) of `DECISIONS.md`; decision D35 of `PLAN.md` (frx7 pinned by revision, a committed path override refused) and `PLAN.md` section 4; the documentation of `cargo fmt`, `cargo clippy`, `cargo test` and `cargo-deny` (its `licenses` check, and the `[graph]` and `[licenses]` tables of `deny.toml`, among them `include-dev`); the documentation of GitHub Actions workflow syntax for the events `push` and `pull_request`. The gates of the frx7 repository are specified the same way in its `docs/spec/gates.md`; a maintainer meets one way of working in both.

Five gates stand between a change and the repository. Each reports a pass, or a failure that names what failed. They run on the maintainer's machine before a push and on a hosted macOS runner after it. The same script runs them in both places. None of them installs or needs a browser or FFmpeg.

## 1. The five gates

| Gate | Short name | Command | Passes when | A failure names |
|---|---|---|---|---|
| Formatting | `format` | `cargo fmt --all -- --check` | no file differs from what `rustfmt` writes | each file that differs |
| Static analysis | `lint` | `cargo clippy --workspace --all-targets -- -D warnings` | no warning and no error, in any crate, test or tool | each message with its place (file, line, column) |
| Tests | `test` | `cargo test --workspace --no-fail-fast` | every test that is not marked (sections 4 and 5) passes | each failing test with the place of its panic (file, line, column), or the error that stopped the build, with its place |
| Licence gate | `licence` | `cargo deny check licenses` | every dependency, build and test dependencies included, is under a licence of the list in `deny.toml` | each dependency with its licence |
| frx7 pin | `pin` | `scripts/gates/frx7.sh check` | frx7 is pinned at one revision, no path override is committed, and a moved pin carries its record (section 9) | each override, with its file and line, or each revision that disagrees |

A gate that builds can also fail because frx7, which Framesmith takes from its private remote, cannot be fetched; the failure then carries one more line that names the revision and the remote (`docs/dev/frx7.md`, section 7).

The licence list is MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, 0BSD, CC0-1.0, Unicode-3.0, Unicode-DFS-2016 and MPL-2.0 (the licences of `CLAUDE.md` rule 11). GPL, AGPL, LGPL and SSPL are not on it, and a dependency with no stated licence, or one cargo-deny cannot read with confidence (a match of at least 0.93), is refused. A dependency under "A OR B" passes when one of the alternatives is on the list. Crates of this workspace are private (`publish = false`) and carry no licence field; they are skipped for that reason alone.

Two lines of `deny.toml` make the gate reach every dependency:

- `include-dev = true` (in `[licenses]`): a dependency used only by tests and tools is checked as well. Without it cargo-deny leaves development dependencies out, and a test dependency under GPL would pass.
- No `targets` line (in `[graph]`): every package in `Cargo.lock` is checked, whatever system it is built for, so a dependency used only on Windows or Linux is refused before those systems are added. The line that named `aarch64-apple-darwin` is dropped.

## 2. The script

`scripts/gates.sh` runs the gates in the order of section 1, always all of those asked for, so that one report shows every failure. For each gate it prints the command, everything the command printed, and a verdict line: `PASS <gate>` or `FAIL <gate>: <what failed>`. It ends with a summary of the five verdicts, and under it a `note:` line for anything that was not run (sections 4 and 5).

| Call | Meaning | Exit code |
|---|---|---|
| `scripts/gates.sh` | all five gates, then the reference machine tests and the yardstick tests (sections 4 and 5) | 0 when all pass, 1 when any fails |
| `scripts/gates.sh --hosted --gate NAME` | one gate as a job of the hosted workflow; the workflow runs the five jobs | 0 when it passes, 1 when it fails |
| `scripts/gates.sh --gate NAME` | one gate, **narrowed** | 3 when it passes, 1 when it fails |
| `scripts/gates.sh -p CRATE` | every gate, the three that take a crate narrowed to it, **narrowed** | 3 when all pass, 1 when any fails |
| wrong usage | the usage text | 2 |

A narrowed run says on its first line and on its last that it is narrowed, and that the gates have not passed. It never exits 0, so that no script and no hook can take it for a pass. The licence gate and the frx7 pin gate always look at the whole workspace; `-p` does not narrow them.

A failure the script itself finds, such as a missing tool, is a failure of the gate that needed it and is named as such. Nothing is skipped silently. The script needs the Rust toolchain and `cargo-deny` (`cargo install --locked cargo-deny --version 0.20.2`); it needs nothing else, and in particular no browser and no FFmpeg.

## 3. Before a push, and after

**Before the push.** `.githooks/pre-push` is a git hook kept in the repository. One command installs it for a clone: `scripts/gates/install-hooks.sh`, which sets `core.hooksPath` to `.githooks`. The hook runs `scripts/gates.sh` whole and exits with its code, so a failing gate stops the push. A push that only deletes branches runs nothing. The gates look at the working tree, not at the commits pushed; the hook says so when the tree has uncommitted changes. `git push --no-verify` passes the hook; the hosted run is the check that cannot be passed that way for `main` and for pull requests.

**After the push.** `.github/workflows/gates.yml` runs on a macOS runner on a push to `main` and on a pull request, and on nothing else (decision A7): work branches rely on the local gates. It has one job for each gate, named `formatting`, `static analysis`, `tests`, `licence gate` and `frx7 pin`, so that the hosted run reports each by name. Each job calls `scripts/gates.sh --hosted --gate NAME`. On a failure the job prints an annotation that names what failed, and writes the same lines to the job summary.

The workflow installs the Rust toolchain named by `rust-toolchain.toml` and, for the licence job, `cargo-deny` at the version named in the workflow. It installs no browser, no FFmpeg and nothing else, and every gate passes without them. Every job that builds (static analysis, tests, licence gate) first runs `scripts/gates/frx7.sh deploy-key` with the Actions secret `FRX7_DEPLOY_KEY`, so that Cargo can fetch frx7 from its private remote (`docs/dev/frx7.md`, section 2); formatting reads no dependency, and the `frx7 pin` job reads git only, fetching all of its history (`fetch-depth: 0`). The hosted tests job does not start the programs of section 5 even where the image has them.

## 4. Reference machine only (decision A27)

A test that needs the hardware encoder or a graphics card cannot be judged on a hosted runner, which is a virtual machine. Such a test is marked in one way, and only one:

1. its name starts with `reference_machine_only_`, and
2. it carries `#[ignore = "reference machine only: <the reason>"]`, where the reason says what it needs (for example "needs the hardware H.264 encoder").

What the marks do:

- A plain `cargo test`, which is what the hosted run executes, does not run the test and prints `test <name> ... ignored, reference machine only: <reason>`. The hosted job copies those lines under the heading "not run (reference machine only)". The test is never reported as passed.
- The local run of `scripts/gates.sh` runs every marked test after the tests of the gate (`cargo test --workspace --no-fail-fast -- --ignored reference_machine_only_`) and counts them. A marked test that fails fails the gate. The count of tests run must equal the count of tests marked; if it does not, the gate fails and says how many were marked and how many ran.
- A test whose name starts with `reference_machine_only_` and which is not ignored fails the gate: it would run on the hosted runner and pass or fail there for the wrong reason.

Framesmith has no such test yet; the rule is in force for the first one (the renderer on the card).

## 5. Yardstick tests

A **yardstick test** is a test that starts a program from outside, as a separate program the developer installed, to compare Framesmith with it. Today the tests of the comparison tool (`crates/parity`) start `ffmpeg` and `ffprobe` to make their test videos and to read them. A yardstick test is marked in one way, and only one:

1. its name starts with `yardstick_`, and
2. it carries `#[ignore = "yardstick: <the reason>"]`.

The name is the one frx7 uses for such tests (its `docs/spec/measure.md`, section 4), so that the gate that runs the core suite without the outside programs can skip them with `--skip yardstick_` in both repositories.

What the marks do:

- **On the hosted run, and wherever the programs are absent, a yardstick test is reported as not run, by name, and never as passed.** A plain `cargo test` prints `test <name> ... ignored, yardstick: <reason>`. The hosted job copies those lines under the heading "not run (yardstick, needs ffmpeg and ffprobe)" and never runs them, even on an image that happens to have the programs. A local run on a machine where `ffmpeg` or `ffprobe` is not in any folder of the search path (looking for them starts nothing) prints the same heading with the same names and ends with a `note:` line that says nothing was compared. That is not a failure of the gate, and it is not a pass of the tests concerned.
- **On a machine that has the programs, the local gates run them**, after the tests of the gate (`cargo test --workspace --no-fail-fast -- --ignored yardstick_`), and count them: the count run must equal the count marked, and a failure fails the gate.
- A test whose name starts with `yardstick_` and which is not ignored with that reason fails the gate.

A test that makes its input with another program is a yardstick test; a test of the product is not allowed to start `ffmpeg` or `ffprobe` at all (`CLAUDE.md` rule 11). The core suite is every test that is not a yardstick test. The gate that proves it passes with only the system's folders on the search path is a separate piece of work.

The marks differ from frx7's in one respect: frx7's yardstick tests run under a runner of its own in its `testkit` crate, which reports "skipped" itself. Framesmith has no such crate; the Rust harness's own `ignore`, read by `scripts/gates.sh`, gives the same three outcomes at the gate (ran and passed, failed, not run by name).

## 6. Tests that stand alone

A gate that fails by chance is no gate. A test never reads a file that another test writes, and never relies on the order the tests run in; the tests of one file run in parallel. A test writes the files it reads, under names of its own.

## 7. Dependencies

Every dependency of Framesmith is recorded in `PROGRESS.md` with its name, licence and reason (`CLAUDE.md` rule 11). `scripts/gates/dependencies.sh list` prints the outside crates of `Cargo.lock` with the licence of each, as `name<TAB>version<TAB>licence`, taken from the manifests in the cargo cache, and checks that the count equals the count of packages with a source in `Cargo.lock` that is not frx7's. The crates of frx7 are ours and are not outside crates: `scripts/gates/dependencies.sh frx7` lists them apart, as `name<TAB>version<TAB>revision`, and checks the count against the packages of `Cargo.lock` whose source is frx7's remote. `scripts/gates/dependencies.sh compare FILE` prints each outside crate whose name does not appear in `FILE` in backticks, and ends non-zero when there is one. The licence gate decides what may be used; the record says why.

## 8. FFmpeg and the browser

No gate fetches, installs or needs FFmpeg or a browser. FFmpeg is a yardstick: tests named as yardstick tests may start a copy the developer installed, as a separate program (section 5). The five gates pass on a machine without it.

## 9. The frx7 pin gate

`scripts/gates/frx7.sh check` is the fifth gate (`pin`). It is specified with the rest of how Framesmith depends on frx7, in `docs/dev/frx7.md` (section 6 has the table of what it looks at and what it refuses); in short:

- **It looks at what is committed**, the tree of `HEAD`, never at the working tree: a maintainer's override in place for local work does not stop the gates, and a committed one cannot pass. It needs `git` and nothing else, and does not reach the remote.
- **It refuses a committed path override and names it**: a `[patch]` or `[replace]` table with a `path` in any `Cargo.toml`, a `path` that is absolute or starts with `..`, a `[patch]`, `paths` or `path` in a tracked `.cargo/config.toml`, and a `Cargo.lock` package with no source that is no crate of this workspace. Each refusal is one line `refused: <file> line <n> commits an override of a dependency: <the text>` and the gate ends with exit code 1.
- **It refuses a pin that is not one stated revision**: more than one `rev`, a `rev` that is not 40 hex digits, a `branch` or `tag`, a lock file that takes frx7 from another source, a history of the pin in `docs/dev/frx7.md` that does not end with the pin.
- **It refuses a pin moved without its record**: a commit that changes the pin must add the row (old revision, new revision, reason) to `docs/dev/frx7.md` and change no file besides `Cargo.toml`, `Cargo.lock` and that document. It reads the whole history, and refuses a shallow clone instead of passing it.
- On a pass it prints `ok:` lines that say what was looked at; a local override in the working tree adds a `note:` line that names `.cargo/config.toml`.

The tests of the gate are `crates/measure/tests/pin_gate.rs` and `crates/measure/tests/local_override_and_deploy_key.rs`; they build small git repositories of their own and run the script in them.
