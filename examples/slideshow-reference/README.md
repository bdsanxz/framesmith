# Slideshow reference (Sprint 24, T004)

Three 160x90 one-second compositions behind one `slideshow`
manifest in a single `composition.kdl`, proving every Sprint 24
outcome end to end: manifest validation with file/ref-named
refusals, a `present` session serving presenter and audience
views in sync off one navigation state machine (keys, branch,
hotspot, autoplay), first-slide-only deck render parity, and a
Studio slideshow panel checklist. The one-command gate is
[`tools/n24-slideshow-proof.sh`](../tools/n24-slideshow-proof.sh).

## Contents

- [`composition.kdl`](composition.kdl) — the reference project:
  compositions `intro`, `agenda`, `detail` (each one 1s text
  card, distinct background and word so frames differ), plus
  `slideshow autoplay=1` with `s1` (intro, notes "welcome",
  fragment at 0.5s, hotspot h1 to s3), `s2` (agenda, notes
  "topics", branch to s3), `s3` (detail, notes "wrap up").
- [`README.md`](README.md) — this file.

## The navigation, by construction

- `next` on s1 holds on the fragment (fragments hold before
  advancing when advanced explicitly); the second `next`
  advances to s2.
- `next` on s2 follows the branch to s3 (branches resolve
  first).
- Hotspot `h1` jumps s1 straight to s3; `home`/`end`/`goto`
  jump; `prev` steps back; `tick` autoplays past the fragment
  by duration once one second has elapsed.
- Navigation serves picker-cache frames: repeated fetches of
  one slide are byte-identical; different slides differ.

## Render parity

`render` on this project (no `--composition`) renders only
the first slide — byte-identical to
`render --composition intro` — with the film's validation
gates applied. No linear deck export exists.
