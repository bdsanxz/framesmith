# Odometer roll: a block built for the catalog

A new Registry block that fills one of the catalog's listed gaps ("data readouts: odometer digit roll"; on 2026-10-01 none of the registry's 394 entries (the registry index; `catalog --json` listed 386 that day) was an odometer; the nearest is the `mk-usage-arc` gauge component). A number rolls into place digit by digit like a mechanical counter: the right-hand digits spin more turns, everything lands by 64 % of the slot, and the number holds still with its label to the end.

```text
registry/blocks/odometer-roll/      the contribution, exactly as it goes into heygen-com/hyperframes
  odometer-roll.html                the composition (oxfmt-formatted)
  registry-item.json                its metadata (validated against the published schema: required keys present, no unknown keys)
compositions/odometer-roll.html     the same file where `hyperframes add` puts it (files[].target)
index.html                          a demo project mounting it twice, the way `add --vars` prints the host
```

`npx hyperframes@0.8.103 check` passes on the demo (0 errors, 0 warnings, 106/106 contrast checks). `npx hyperframes@0.8.103 render -c compositions/odometer-roll.html` renders the block alone at 5.0 s, 1920×1080.

**Params are declared variables.** The block declares seven variables on its `<html>` (`value`, `prefix`, `suffix`, `label`, `ink`, `ground`, `accent`), each with a default. Studio shows them as a customization panel, and `npx hyperframes@0.8.103 add odometer-roll --vars '{"value":394,…}'` validates them against the declaration (undeclared keys are reported as ignored) and bakes them into the host as `data-variable-values`. The block's script reads them once at init with `window.__hyperframes.getVariables()` (the digit count depends on the value), and the label binds with `data-var-text`. Instance 2 of the demo (`value 394`, light ground, orange accent) proves per-instance values reach the block's JS.

How it meets the registry rules:
- **Deterministic and seekable**: no randomness or clock; one paused timeline registered under the host's `data-composition-id`, with timing scaled from the slot's `data-duration`.
- **Prefixed ids** (`od-…`) so two instances, or other blocks, never collide.
- **Monochrome placeholder**: windows and separators are alpha steps of the ink (8 %, 14 %, 45 %, 72 %); the accent marks one element, the rule under the number.
- **Motion bar**: one hero (the number), no constant-rate motion (`power4.out` rolls), and the key information holds still for 1.4 s before the end.
- The reels are taller than their windows by design, so each reel carries `data-layout-allow-overflow`. Without it, `check` reported 19 `escaped_container` / `canvas_overflow` warnings.

What is left for the contributor (it needs their GitHub account and is outward-facing, so it is theirs to run): fork and branch, copy `registry/blocks/odometer-roll/` in, regenerate `registry/registry.json` with the repo's script, generate the catalog page, `npx hyperframes@0.8.103 publish` for a reviewer preview link, then open the PR with the preview MP4 attached. The full sequence is in `references/contributing-to-the-catalog.md`.

Testing an install from a local registry does not work: `hyperframes.json` `"registry"` must be an absolute `https://` URL ("Registry URLs must be an absolute https:// URL…"). Test the block by copying it to its `files[].target` and mounting the snippet, as this demo does.
