# The quality gates of the Framesmith repository

Status: in force from Sprint 1. Sources: `CLAUDE.md` rule 11 (licences, and where hosted jobs run); decisions A7 (hosted jobs on `main` and on pull requests only) and A27 (tests that need the hardware encoder or a card) of `DECISIONS.md`; decision D35 of `PLAN.md` (frx7 pinned by revision, a committed path override refused) and `PLAN.md` section 4; the documentation of `cargo fmt`, `cargo clippy`, `cargo test` and `cargo-deny` (its `licenses` check, and the `[graph]` and `[licenses]` tables of `deny.toml`, among them `include-dev`); the documentation of GitHub Actions workflow syntax for the events `push` and `pull_request` and for the files a step may write to (`GITHUB_STEP_SUMMARY`, `GITHUB_OUTPUT`, `GITHUB_ENV`, `GITHUB_PATH`); the manual pages `sandbox-exec(1)`, `sandbox(7)`, `log(1)` and `xcode-select(1)` of macOS; the options of the Rust test harness (`--list`, `--skip`, `--exact`, `--ignored`, `--include-ignored`) and the Cargo Book (`target.<cfg>.runner`, `harness = false`); `CLAUDE.md` rule 11 (FFmpeg is never needed to build or to test; a yardstick test may start it from outside) and decision A29 (the method of watching a process). The gates of the frx7 repository are specified the same way in its `docs/spec/gates.md`; a maintainer meets one way of working in both.

Six gates stand between a change and the repository. Each reports a pass, or a failure that names what failed. They run on the maintainer's machine before a push and on a hosted macOS runner after it. The same script runs them in both places. None of them installs or needs a browser or FFmpeg.

## 1. The six gates

| Gate | Short name | Command | Passes when | A failure names |
|---|---|---|---|---|
| Formatting | `format` | `cargo fmt --all -- --check` | no file differs from what `rustfmt` writes | each file that differs |
| Static analysis | `lint` | `cargo clippy --workspace --all-targets -- -D warnings` | no warning and no error, in any crate, test or tool | each message with its place (file, line, column) |
| Tests | `test` | `cargo test --workspace --no-fail-fast` | every test that is not marked (sections 4 and 5) passes, and a test named `reference_machine_only_` or `yardstick_` carries its mark | each failing test with the place of its panic (file, line, column), or the error that stopped the build, with its place |
| Core suite without FFmpeg | `core` | `scripts/gates/core-selftest.sh`, then `scripts/gates/core-suite.sh` | the control fails each test that misbehaves on purpose by name, and the core suite passes with only the system's folders on the search path (section 10) | each failing test, each core test file that names `ffmpeg` or `ffprobe`, each program the system refused to start, and each failure of the control |
| Licence gate | `licence` | `cargo deny check licenses` | every dependency, build and test dependencies included, is under a licence of the list in `deny.toml` | each dependency with its licence |
| frx7 pin | `pin` | `scripts/gates/frx7.sh check` | frx7 is pinned at one revision, no path override is committed, and a moved pin carries its record (section 9) | each override, with its file and line, or each revision that disagrees |

A gate that builds can also fail because frx7, which Framesmith takes from its private remote, cannot be fetched; the failure then carries one more line that names the revision and the remote (`docs/dev/frx7.md`, section 7).

The licence list is MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, 0BSD, CC0-1.0, Unicode-3.0, Unicode-DFS-2016 and MPL-2.0 (the licences of `CLAUDE.md` rule 11). GPL, AGPL, LGPL and SSPL are not on it, and a dependency with no stated licence, or one cargo-deny cannot read with confidence (a match of at least 0.93), is refused. A dependency under "A OR B" passes when one of the alternatives is on the list. Crates of this workspace are private (`publish = false`) and carry no licence field; they are skipped for that reason alone.

Two lines of `deny.toml` make the gate reach every dependency:

- `include-dev = true` (in `[licenses]`): a dependency used only by tests and tools is checked as well. Without it cargo-deny leaves development dependencies out, and a test dependency under GPL would pass.
- No `targets` line (in `[graph]`): every package in `Cargo.lock` is checked, whatever system it is built for, so a dependency used only on Windows or Linux is refused before those systems are added. The line that named `aarch64-apple-darwin` is dropped.

## 2. The script

`scripts/gates.sh` runs the gates in the order of section 1, always all of those asked for, so that one report shows every failure. For each gate it prints the command, everything the command printed, and a verdict line: `PASS <gate>` or `FAIL <gate>: <what failed>`. It ends with a summary of the six verdicts, and under it a `note:` line for anything that was not run (sections 4 and 5).

| Call | Meaning | Exit code |
|---|---|---|
| `scripts/gates.sh` | all six gates, then the reference machine tests and the yardstick tests (sections 4 and 5) | 0 when all pass, 1 when any fails |
| `scripts/gates.sh --hosted --gate NAME` | one gate as a job of the hosted workflow; the workflow runs the six jobs | 0 when it passes, 1 when it fails |
| `scripts/gates.sh --gate NAME` | one gate, **narrowed** | 3 when it passes, 1 when it fails |
| `scripts/gates.sh -p CRATE` | every gate, the three that take a crate narrowed to it, **narrowed** | 3 when all pass, 1 when any fails |
| wrong usage | the usage text | 2 |

A narrowed run says on its first line and on its last that it is narrowed, and that the gates have not passed. It never exits 0, so that no script and no hook can take it for a pass. The licence gate and the frx7 pin gate always look at the whole workspace; `-p` does not narrow them.

A failure the script itself finds, such as a missing tool, is a failure of the gate that needed it and is named as such. Nothing is skipped silently. The script needs the Rust toolchain and `cargo-deny` (`cargo install --locked cargo-deny --version 0.20.2`); it needs nothing else, and in particular no browser and no FFmpeg.

## 3. Before a push, and after

**Before the push.** `.githooks/pre-push` is a git hook kept in the repository. One command installs it for a clone: `scripts/gates/install-hooks.sh`, which sets `core.hooksPath` to `.githooks`. The hook runs `scripts/gates.sh` whole and exits with its code, so a failing gate stops the push. A push that only deletes branches runs nothing. The gates look at the working tree, not at the commits pushed; the hook says so when the tree has uncommitted changes. `git push --no-verify` passes the hook; the hosted run is the check that cannot be passed that way for `main` and for pull requests.

**After the push.** `.github/workflows/gates.yml` runs on a macOS runner on a push to `main` and on a pull request, and on nothing else (decision A7): work branches rely on the local gates. It has one job for each gate, named `formatting`, `static analysis`, `tests`, `core suite without FFmpeg`, `licence gate` and `frx7 pin`, so that the hosted run reports each by name. Each job calls `scripts/gates.sh --hosted --gate NAME`. On a failure the job prints an annotation that names what failed, and writes the same lines to the job summary.

The workflow installs the Rust toolchain named by `rust-toolchain.toml` and, for the licence job, `cargo-deny` at the version named in the workflow. It installs no browser, no FFmpeg and nothing else, and every gate passes without them. Every job that builds (static analysis, tests, core suite, licence gate) first runs `scripts/gates/frx7.sh deploy-key` with the Actions secret `FRX7_DEPLOY_KEY`, so that Cargo can fetch frx7 from its private remote (`docs/dev/frx7.md`, section 2); formatting reads no dependency, and the `frx7 pin` job reads git only, fetching all of its history (`fetch-depth: 0`). The hosted tests job does not start the programs of section 5 even where the image has them.

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

A **yardstick test** is a test that starts a program from outside, as a separate program the developer installed, to compare Framesmith with it. There are two programs: FFmpeg (`ffmpeg` and `ffprobe`, which today make the test videos of the comparison tool `crates/parity` and read them) and the reference (the black box installed outside the repository, which `crates/parity` runs). A yardstick test is marked in one way, and only one:

1. its name starts with `yardstick_`, and a test that needs the reference starts with `yardstick_reference_`;
2. it sits in a test file, or a folder of a test file, whose name starts with `yardstick` (section 10, the placement rule); and
3. it is run by the runner of `measure::yardstick`, not by the Rust test harness: its test target has `harness = false` in the manifest and its `main` is `measure::yardstick_tests!`. The runner exists because the Rust harness has two outcomes only (passed, failed), and a comparison whose program is absent is a third: skipped.

The name is the one frx7 uses for such tests (its `docs/spec/measure.md`, section 4), so that the gate that runs the core suite without the outside programs skips them with `--skip yardstick_` in both repositories.

**The runner takes the options of the Rust test harness** that matter here and ignores the others: names to match (a part of a name, or the whole with `--exact`), `--skip <part>` (any number), `--list`, `--ignored`, `--include-ignored`. It sorts the tests by name and runs them one at a time, each on a thread of its own name, so that a failure is named at its panic like any other test.

**The outcomes.** Every line has the form of the Rust harness, `test <name> ... <outcome>`:

| Case | Line | Counts as |
|---|---|---|
| No `--ignored` and no `--include-ignored` (a plain `cargo test`) | `test <name> ... ignored, yardstick: needs <program>, which it starts from outside` | not run, listed by name; never passed |
| Asked to run; the program is absent | `test <name> ... skipped, <program> was not found <where>; nothing was compared` | skipped; never passed, never failed |
| Asked to run; the program is present and the test passes | `test <name> ... ok` | passed |
| Asked to run; the program is present and the test fails | `test <name> ... FAILED`, then under `failures:` the test's name | failed |

`<program>` is `ffmpeg and ffprobe` or `the reference`. FFmpeg is absent when `ffmpeg` or `ffprobe` is not a file in any folder of the search path (looking for them starts nothing); the line says `ffmpeg was not found in any folder of the search path` (or `ffprobe`, or `ffmpeg and ffprobe were not found ...`) and names what is missing. The reference is absent when its command is not in its folder outside the repository, or when the program its command is a script of (`node`) is not in a folder of the search path; the line says `the reference is not installed under <folder>` or names the missing runtime (the reference's rule is one function, `parity::blackbox::absence`, tested without starting any program in `crates/parity/tests/reference_absence.rs`). The last line of a run is `test result: ok. ...` when something ran and nothing failed or was skipped, `test result: skipped. 0 passed; 0 failed; <n> skipped; ...` when nothing ran and something was skipped, `test result: FAILED. ...` when any failed. The exit code is 0 unless a test failed (101, as the Rust harness has it). **A run that skipped everything therefore ends with success and says plainly that nothing was compared: its first word is `skipped`, never `ok`.**

**Running them.** A plain `cargo test --workspace` lists each yardstick test as ignored and runs none. `cargo test --workspace -- --ignored yardstick_` runs them: with the programs present they compare, without them each is reported as skipped by name. `scripts/gates.sh` does this after the tests of the gate (not on the hosted run, which lists them as not run, by name, and never starts one, even on an image that happens to hold the programs) and counts them: tests run and passed plus tests skipped must equal the tests marked; a failure fails the gate; skipped tests are not a failure and are named under the heading "skipped (yardstick)" with a `note:` line that says nothing was compared and that they are not passed.

A test that makes its input with another program is a yardstick test; a test of the product is not allowed to start `ffmpeg` or `ffprobe` at all (`CLAUDE.md` rule 11). The core suite is every test that is not a yardstick test (section 10).

## 6. Tests that stand alone

A gate that fails by chance is no gate. A test never reads a file that another test writes, and never relies on the order the tests run in; the tests of one file run in parallel. A test writes the files it reads, under names of its own.

## 7. Dependencies

Every dependency of Framesmith is recorded in `PROGRESS.md` with its name, licence and reason (`CLAUDE.md` rule 11). `scripts/gates/dependencies.sh list` prints the outside crates of `Cargo.lock` with the licence of each, as `name<TAB>version<TAB>licence`, taken from the manifests in the cargo cache, and checks that the count equals the count of packages with a source in `Cargo.lock` that is not frx7's. The crates of frx7 are ours and are not outside crates: `scripts/gates/dependencies.sh frx7` lists them apart, as `name<TAB>version<TAB>revision`, and checks the count against the packages of `Cargo.lock` whose source is frx7's remote. `scripts/gates/dependencies.sh compare FILE` prints each outside crate whose name does not appear in `FILE` in backticks, and ends non-zero when there is one. The licence gate decides what may be used; the record says why.

## 8. FFmpeg and the browser

No gate fetches, installs or needs FFmpeg or a browser. FFmpeg is a yardstick: tests named as yardstick tests may start a copy the developer installed, as a separate program (section 5). The six gates pass on a machine without it.

## 9. The frx7 pin gate

`scripts/gates/frx7.sh check` is the fifth gate (`pin`). It is specified with the rest of how Framesmith depends on frx7, in `docs/dev/frx7.md` (section 6 has the table of what it looks at and what it refuses); in short:

- **It looks at what is committed**, the tree of `HEAD`, never at the working tree: a maintainer's override in place for local work does not stop the gates, and a committed one cannot pass. It needs `git` and nothing else, and does not reach the remote.
- **It refuses a committed path override and names it**: a `[patch]` or `[replace]` table with a `path` in any `Cargo.toml`, a `path` that is absolute or starts with `..`, a `[patch]`, `paths` or `path` in a tracked `.cargo/config.toml`, and a `Cargo.lock` package with no source that is no crate of this workspace. Each refusal is one line `refused: <file> line <n> commits an override of a dependency: <the text>` and the gate ends with exit code 1.
- **It refuses a pin that is not one stated revision**: more than one `rev`, a `rev` that is not 40 hex digits, a `branch` or `tag`, a lock file that takes frx7 from another source, a history of the pin in `docs/dev/frx7.md` that does not end with the pin.
- **It refuses a pin moved without its record**: a commit that changes the pin must add the row (old revision, new revision, reason) to `docs/dev/frx7.md` and change no file besides `Cargo.toml`, `Cargo.lock` and that document. It reads the whole history, and refuses a shallow clone instead of passing it.
- On a pass it prints `ok:` lines that say what was looked at; a local override in the working tree adds a `note:` line that names `.cargo/config.toml`.

The tests of the gate are `crates/measure/tests/pin_gate.rs` and `crates/measure/tests/local_override_and_deploy_key.rs`; they build small git repositories of their own and run the script in them.

## 10. The core suite and the gate that runs it (`core`)

The **core suite** is every test of the workspace that is not a yardstick test: its unit tests and its integration tests (the tests in documentation comments, which `rustdoc` runs outside any runner, are left to the test gate, and the workspace has none). `CLAUDE.md` rule 11 says the core suites pass on a machine without FFmpeg. The core gate runs them the way such a machine runs them, and shows that a later change which starts `ffmpeg`, `ffprobe` or the reference from a core test fails the gate and is named.

**With only the system's folders on the search path.** The search path is `/usr/bin:/bin:/usr/sbin:/sbin` and nothing else: these four folders are "the system's folders" (the same value as frx7's `docs/spec/measure.md`, section 5). The gate runs

```
cargo test --workspace --lib --bins --tests --no-fail-fast --config "target.'cfg(all())'.runner=[core-runner.sh]" -- --skip yardstick_
```

so that cargo starts every test program through `scripts/gates/core-runner.sh`, which sets the search path and starts the program under a system sandbox profile (`sandbox-exec`) that forbids starting any program except those of four places:

1. the programs the workspace built, under its `target` folder;
2. the folders of the system's tools listed in `scripts/gates/core-allowed.txt` (one absolute folder on each line, each inside `/usr/bin`, `/bin`, `/usr/sbin`, `/sbin`, `/usr/libexec` or `/System`; a line outside them is an error of the runner), where `git`, `bash`, `ssh-keygen` and the like are;
3. the folder of the developer tools that `xcode-select -p` names, because `/usr/bin/git` is a stub that starts the real `git` there; and
4. the folder of the Rust toolchain that runs the suite (`rustc --print sysroot`), because two tests start `cargo` by the absolute path cargo gives them in the variable `CARGO` (they build a workspace whose frx7 pin cannot be fetched). A test that starts a program of the toolchain puts the toolchain's folder on the search path of that child itself; the search path of the test is still the system's four folders.

Nothing else may start: `ffmpeg` and `ffprobe` are not in these places on any machine (Homebrew puts them in `/opt/homebrew/bin`), and neither is the reference, which is installed outside the repository. A start the profile forbids fails in the test that made it with "Operation not permitted"; a start by name that finds no program fails with "No such file or directory". Either way the gate reports `test <name> failed (<file:line:column of the panic>)`, which names the test, and then, when the system log can be read (`log show`, for the window of the run), `the system refused to start <program>`. A refused start is never retried or let through. In the local run the gate then runs the tests marked "reference machine only" the same way and counts them, as section 4 does; the hosted run does not run them.

**Placement.** A core test file does not name `ffmpeg` or `ffprobe`. `scripts/gates/core-suite.sh` reads every `.rs` file under `crates/*/tests/`, at any depth, except those with a folder or a file name that starts with `yardstick`, and reports each line that names either (in any case) with the file, the line and the test function around it, as `core problem: ...`. This finds what the profile cannot: a test that starts `ffmpeg` by name and takes "not found" for success starts nothing the profile can see.

**The control.** Before the real suite, `scripts/gates/core-selftest.sh` makes a small workspace of its own, of tests that misbehave on purpose, and runs the machinery above on it: a test that starts `ffmpeg` by name, one that starts `ffprobe` by name, one that starts a file named `ffmpeg` by its path, and one that starts a program of the reference's kind (a command in a folder outside the target and the system's) must each be failed by name; a test that starts a system tool, a test that takes "not found" for success and a test that starts nothing must pass; and the placement rule must name the test that takes "not found" for success. The gate fails when the control does not come out so, because a trap that no longer closes would pass for ever. The control and the real suite are one gate, `core`; the control's deliberate failures are printed under the gate's output and appear nowhere else (section 12).

## 11. A skipped comparison is not a passed one

A comparison that did not run proves nothing about the line of `ROADMAP.md` it was meant to support. The tools say so in their own words, so that no one can copy "passed" from them:

- A yardstick test whose program is absent is `skipped`, never `ok`, and the run that skipped all of them ends `test result: skipped` (section 5).
- The comparison tool says `result: not run` when the reference is not installed, prints no word `passed`, and ends with its own exit code 3 (`crates/parity/src/report.rs`).
- `scripts/gates.sh` never counts a skipped yardstick test in "ran and passed", and says in a `note:` that nothing was compared.

A line of `ROADMAP.md` is marked `[x]` only for a comparison that ran and passed on the final code (`CLAUDE.md` rule 2); an evidence file that shows `skipped` or `not run` is not evidence for it, and the person who ticks the line reads the result line of the run first.

## 12. A script a test starts reports to no runner

Some tests start `scripts/gates.sh` (a copy, in a scratch folder, that is made to fail on purpose) or the machinery of the core gate. On a hosted runner such a script finds `GITHUB_ACTIONS=true` and the files `GITHUB_STEP_SUMMARY`, `GITHUB_OUTPUT`, `GITHUB_ENV` and `GITHUB_PATH` in its environment, and would write its deliberate failure into the summary of a job that passed. So **every helper of the tests that starts a script or a program of the repository removes these five variables from the child's environment** (`isolate_from_runner` in `crates/measure/tests/common/mod.rs`), and the core gate's control and its scratch workspace do the same. The test `a_gate_started_by_a_test_leaves_the_runners_summary_as_it_was` runs the gate test again with those variables set, to a file of its own, and requires the file to be as it was.
