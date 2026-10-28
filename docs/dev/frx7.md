# How Framesmith depends on frx7

Status: in force from Sprint 1. Sources: decision D35 of `PLAN.md` (a pinned revision, a path override for work on both at once) and `PLAN.md` section 4; decision B51 of `DECISIONS.md` (frx7's remote is public, so a fetch needs no credentials), decisions A7 (hosted jobs on `main` and on pull requests only) and A27 of `DECISIONS.md`; `CLAUDE.md` rules 10 and 11 (frx7 is a repository of its own and meets Framesmith only at its public interfaces; every dependency passes the licence gate); the documentation of Cargo for dependencies from a git repository (the `git` and `rev` keys), for `[patch]` and for the configuration keys `net.git-fetch-with-cli`; The gates are specified in `docs/dev/gates.md`; this file is the part of them that concerns frx7.

frx7 is the media stack, a repository of its own with a public remote, `https://github.com/bdsanxz/frx7`. Framesmith reaches it at one stated revision of that remote. A fresh copy of the Framesmith repository builds on a machine that has no frx7 folder anywhere: Cargo fetches the revision. A maintainer who changes both repositories at once builds against the local folder instead, with an override that is never committed.

## 1. The pin

The pin is the `rev` of the frx7 entries in the `[workspace.dependencies]` table of the root `Cargo.toml`. An entry is on one line:

```
base = { git = "https://github.com/bdsanxz/frx7", rev = "<40 hex digits>" }
```

- **One revision.** Every frx7 entry has the same `rev`, the full 40-digit name of a commit of frx7's remote. No `branch`, no `tag` and no short name: only a full commit name cannot move.
- **One spelling of the remote.** `https://github.com/bdsanxz/frx7`, exactly, in every manifest. Nothing rewrites it: the remote is public, so every builder fetches it over `https` (section 2).
- **Only the root names frx7.** A member crate takes frx7's crates as `{ workspace = true }`. The crates of frx7 have plain names (`base`, `format`, `mp4`, `probe`, ...) and the entry is named like the crate.
- **Only what is used.** An entry is added by the order that first uses the crate, at the pinned revision. A dependency no crate uses is not resolved and is never fetched. frx7's own inner dependencies (`mp4` needs `base` and `format`) are taken from the same revision without an entry of their own.
- **The lock file agrees.** `Cargo.lock` names each frx7 package with the source `git+https://github.com/bdsanxz/frx7?rev=<pin>#<pin>`. Cargo writes this; nobody edits it.
- **The record agrees.** The history in section 5 ends with the pin.

The pin today is the first row of the history in section 5. It is the head of frx7's `main` on the day the pin was first made, a commit that passed frx7's gates locally and on its hosted run. The workspace's `PROGRESS.md` records it too.

## 2. Fetching it

Cargo fetches the revision over `https`, with no credentials. frx7's remote is public (decision B51), so whoever builds needs nothing set up: a maintainer's machine, a hosted runner and a fresh copy of the repository fetch `base`, `format`, `mp4` and `probe` from the remote alike. A repeat build uses Cargo's cache.

The URL is `https` in every manifest, as section 1 says; no rewrite of it is made anywhere.

## 3. The licence gate and the list of dependencies

frx7's crates are ours and carry the licence GPL-3.0-or-later of the workspace (decision B51). They are unpublished (`publish = false`), and `cargo deny` skips unpublished crates as it skips Framesmith's own, and `deny.toml` is unchanged; the outside crates they might depend on would be checked like any other. `scripts/gates/dependencies.sh list` prints the outside crates of `Cargo.lock` and does not list frx7's crates among them; `scripts/gates/dependencies.sh frx7` lists them apart, as `name<TAB>version<TAB>revision`, so that the record of dependencies says which revision of frx7 a build took.

## 4. The local override

A maintainer who works on both repositories builds Framesmith against the local frx7 folder:

```
scripts/gates/frx7.sh override /path/to/frx7      # writes .cargo/config.toml
cargo build -p measure                            # frx7 is now compiled from /path/to/frx7
scripts/gates/frx7.sh drop-override               # back to the pin
```

- `override` writes `.cargo/config.toml` with one `[patch."https://github.com/bdsanxz/frx7"]` entry, a `path` into the local folder, for each frx7 crate the build takes: the crates named in the root `Cargo.toml` and the crates they need, found by reading the local folder's manifests. It refuses a folder that is not an frx7 workspace, and an existing `.cargo/config.toml` it did not write. It writes nothing in the frx7 folder, and neither does Cargo.
- `.cargo/.gitignore` keeps `.cargo/config.toml` out of git, so the override is not committed by accident. Cargo reads the file from the repository's folder and every folder above it.
- Cargo records a patched package in `Cargo.lock` without a source. The lock then differs from the committed one: that difference stays in the working tree and is not committed. The next build after `drop-override` writes the pinned sources back by itself, and `git checkout -- Cargo.lock` does the same at once.
- A crate of frx7 that is new to the build is added to the override by running `override` again after removing the old one (`drop-override`, then `override`).

## 5. Moving the pin

Moving the pin is a change of its own, made after the frx7 change it follows has passed frx7's gates and is on its remote (frx7's `README.md` says the same from its side).

1. Take the full name of the new commit of frx7's `main` (`git ls-remote https://github.com/bdsanxz/frx7 main`).
2. In one commit, change the `rev` of every frx7 entry of the root `Cargo.toml`, let Cargo rewrite `Cargo.lock`, and add one row at the end of the table below: the date, the old revision, the new revision and the reason. Change nothing else in that commit.
3. Run the gates. The gate "frx7 pin" reads the history (section 6).

If the new frx7 needs a change to Framesmith to build, that change is a commit of its own beside the pin's, on the same branch, never in it.

### The pin's history

Oldest row first. A row's old revision is the previous row's new one; the first row's old revision is `none`. A reason is one sentence, with no vertical bar in it.

| Date | Old revision | New revision | Reason |
|---|---|---|---|
| 2026-10-14 | `none` | `23b07bc0c72771c9406461fedbe2ae89ffcf2816` | First pin: the head of frx7's `main` on the day Framesmith was first built against it; it passed frx7's local gates and its hosted run 38476900117, and it holds the container reader and writer and the probe the first use needs (section 8). |
| 2026-10-14 | `23b07bc0c72771c9406461fedbe2ae89ffcf2816` | `d866734668a6ceefd694a72d60db413f17ca6c66` | The head of frx7's `main` on 2026-10-14; it passed frx7's local gates and the five jobs of its hosted run 38495049627, and it holds the encoding and decoding of H.264 through the system's codecs behind `codec`'s interfaces and the reading of H.264 and HEVC headers by `probe`, which the comparison of a render with the reference's render needs. |
| 2026-10-14 | `d866734668a6ceefd694a72d60db413f17ca6c66` | `495d0764b41a9ae6fbe7ad8b7e00cd031c6b571e` | The head of frx7's `main` on 2026-10-14 after its T016 was merged; it holds AAC written through the system's encoder at a constant bit rate and read back to the sample by its edit list, which the sound track of every render needs. |
| 2026-10-18 | `495d0764b41a9ae6fbe7ad8b7e00cd031c6b571e` | `f43e7c017ed395186e0b993345e87eaed780f33c` | The head of frx7's `main` on 2026-10-18 after its order t048s was merged, which reads a still picture from bytes held in memory, as the emoji glyphs of a text need (decision B24 of `DECISIONS.md`); the pinned commit is frx7's `main` as pushed on 2026-10-18, and it passed the local gates of that push (with the 35 tests marked reference machine only); those are the gates of frx7's tree, not the gates of this order's tree; its two hosted runs of 2026-10-18, `gates` (run 39099653068) and `system-free parts on other systems` (run 39099653075), were refused by the hosted account before any step ran, for a failed payment or a spending limit, so they show no result on the code. |
| 2026-10-26 | `f43e7c017ed395186e0b993345e87eaed780f33c` | `00114c411204178cfdf502e14f889b39062c87dc` | The head of frx7's `main` on 2026-10-26 after its order wavwriter was merged; it brings frx7's library events into Framesmith's log (B29), frx7's gate control (W6), and `testkit`'s test-only writers of sound files, which T080 needs. |

## 6. The gate "frx7 pin"

`scripts/gates/frx7.sh check` is the fifth gate of `scripts/gates.sh` (`pin`) and has a job of its own in the hosted workflow. It looks at what is committed (the tree of `HEAD`), never at the working tree, so that an override in place for local work does not stop a maintainer's gates, and a committed one cannot pass. It fails with one line `refused: ...` for each finding, each naming the file, the line where there is one, and the override or the revision. It needs `git` and nothing else; it does not reach the remote.

Committed content it looks at, and what it refuses:

| Where | Refused | Names |
|---|---|---|
| Every `Cargo.toml` | a `[patch...]` table with a `path`, a `[patch...]` table that names frx7, any `[replace]` table, and any `path` that is absolute or starts with `..` (a path that leaves the repository) | the file, the line, the override |
| Every tracked `.cargo/config` and `.cargo/config.toml` | a `[patch...]` table, a `paths` key or any `path` | the file, the line, the override |
| `Cargo.lock` | a package with no source that is not a crate of this workspace (where a path override stood when the lock was written); an frx7 package whose source is not the pin; no frx7 package at all while the manifests ask for some | the package and its source |
| The frx7 entries of the manifests | more than one `rev`; a `rev` that is not 40 hex digits; a `branch` or `tag`; a remote not spelled as in section 1; no entry; an entry outside the root | the file, the line |
| `docs/dev/frx7.md` | a history whose last row's new revision is not the pin, whose rows do not chain, or whose reasons are empty | the row |
| The history of `Cargo.toml` (every commit that is not a merge) | a commit that moves the pin (its old and new revisions both exist and differ) without adding the row for that move, or that changes a file besides `Cargo.toml`, `Cargo.lock` and `docs/dev/frx7.md`; the commit that first sets the pin without a row for it | the commit and the revisions |

The gate reads the whole history, so a shallow clone is refused with a message that says so; the hosted job fetches all of it. A working tree that carries a local override passes, with a `note:` line that names the file.

## 7. When the pin cannot be fetched

When Cargo cannot fetch the revision, the build fails. Cargo's message names the remote and the revision on its `unable to update` line, for example

```
unable to update https://github.com/bdsanxz/frx7?rev=<revision>
```

followed by the cause (the revision is not on the remote, or the remote does not answer). `scripts/gates.sh` adds one line to the failure of any gate whose output holds that message: `frx7 revision <revision> could not be fetched from <remote>; see docs/dev/frx7.md, section 7`. The causes:

| Cause | What to do |
|---|---|
| The revision is not on the remote (a wrong pin, or history rewritten) | Move the pin (section 5) to a revision that is on `main`. |
| The remote does not answer | Run the build again; for a maintainer, `git ls-remote https://github.com/bdsanxz/frx7` shows whether git reaches it. |

## 8. The first use

The crate `crates/measure` (development only) makes a short clip with frx7's own writer and reads its facts back with frx7, with no `ffmpeg` and no `ffprobe`. Its interface uses its own types (`ClipSpec`, `ClipFacts`), not frx7's. The clip is an MP4 file whose track holds the facts of a film (codec name, size, frame rate, number of pictures, duration) and placeholder packets: frx7's writer takes packets that are already encoded, and no encoder is used, so the clip cannot be decoded. It serves a test that needs a file with known container facts. The suite's decodable clips are the next order's.

- `write_clip(path, &ClipSpec)` writes the file through the MP4 format of frx7's container registry.
- `clip_facts(path)` opens it through the same registry and gives the container, the codec, the size, the rate, the number of pictures, the duration and the byte count.

## 9. What this file does not cover

Publishing frx7 as a package, and installing the `frx7` command with Framesmith, are not part of this work.
