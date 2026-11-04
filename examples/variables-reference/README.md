# Variables reference (Sprint 9, T007)

One composition proving every Sprint 9 outcome end to end: typed
variables, `var(--id)` bindings, per-render overrides, strict
validation, named themes, and batch rendering.

## Contents

- [`composition.kdl`](composition.kdl) — the reference composition.
- [`rows-pass.json`](rows-pass.json) — three valid batch rows.
- [`rows-mixed.json`](rows-mixed.json) — two valid rows plus one bad
  row (`align` outside its `options`), for the per-row summary and
  fail-fast proof.

## The composition

Nine declarations cover all seven variable types, each bound:

| id | type | binds |
| --- | --- | --- |
| `title` | string | text content |
| `count` | number | text `x` attribute and tween `to` (motion value) |
| `accent` | color | text `color`, box `fill` |
| `show` | boolean | text `hidden` |
| `align` | enum (`left, center, right`) | text `align` |
| `brand` | font | text `font-family` |
| `hero` | image | image `path` (media) |
| `spare` | string, never bound | strict-mode unused-declaration refusal |
| `tagline` | string, no default | inline-fallback (`var(--tagline, Fresh daily)`) and strict supply-the-value refusal |

Two themes (`brand`, `bold`) each carry `accent` and `headline`
tokens; `headline` is token-only (no variable declaration). The
composition selects `theme="brand"`; `--theme bold` re-skins every
bound token. Precedence is element literal, then `--variables`,
then theme token, then default, then `var()` fallback.

## Generated assets (not checked in)

`hero.png` / `hero-alt.png` (via `ffmpeg lavfi testsrc`/`color`)
and `fonts/Body.ttf` (copied from a system font) are staged by the
proof script, because fonts cannot be redistributed and stills stay
byte-stable only when generated with the proving toolchain.

## Prove it

```sh
sh framesmith/tools/n09-variables-proof.sh
```

One command stages this reference to a temp dir, renders every
variant twice, checks SHA-256 byte-identity per variant, asserts
every refusal (strict stale key, strict unused declaration,
invalid override, unknown theme, bad batch row), and reports
`PROOF: PASS` only when every check succeeds (non-zero otherwise).
Rendered variant MP4s plus the proof log land in
`evidences/proof-n09-T007/`.
