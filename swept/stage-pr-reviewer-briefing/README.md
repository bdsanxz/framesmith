# PR reviewer briefing — heygen-com/hyperframes#2026

The PR-to-video prompt, in this skill's generic form, pointed at heygen-com/hyperframes#2026:

**Media.** This inspiration ships without its media files. In the compositions each photo or video is a labelled stand-in (its `data-media` names the file it held; `data-media-kind="video"` marks a clip), each sound a `SOUND SLOT` comment, and each font a `local()` face; the guide comment at the top of each composition says how to plug a real file back in. The files named below are the ones the film was made with. `CREDITS.md` names the source of each.

```text
PR explainer. https://github.com/heygen-com/hyperframes/pull/2026
This PR is open and waiting on review. Make a ~60-second reviewer briefing.
```

This inspiration is the director's side of that run. Its look (the `code-editorial` preset's palette and type) is written straight into the compositions; a new project keeps it in `frame.md` (SKILL.md step 3).

- `STORYBOARD.md`: the full 60s plan written from the real PR (`gh pr view`, `gh pr diff`). Archetype, per-frame `type`, voiceover, persuasion, and a `### Source excerpt` for the code beat.
- Three frames built and cut together into a 14s proof (`index.html`): hook (0–4s), the `code-diff` registry block filled with the real hunk (4–10s), and the handoff (10–14s). `npx hyperframes@0.8.103 check` passes on `hyperframes@0.8.103`.

How the code beat was made:

```bash
npx hyperframes@0.8.103 add code-diff
npm install --no-save shiki
node ../../scripts/shiki-tokens.mjs --seq diff --lang ts before.ts after.ts --inject compositions/code-diff.html
```

`before.ts` and `after.ts` were temporary files holding the hunk's old and new code from the PR; the tokens they produced live in `compositions/code-diff.html`.

Then the block's filename label was set to the real file, and its `.editor` was changed from `left/top 50% + translate(-50%,-50%)` to `inset: 0; margin: auto`. Mounted as a sub-composition, the block's GSAP `scale` entrance replaced the CSS translate and pushed the editor to the bottom-right. `check` did not flag it because the block marks the editor `data-layout-ignore`; only the snapshot showed it.

The handoff uses a monogram instead of `assets/vanceingalls.png` so the inspiration ships without binaries.
