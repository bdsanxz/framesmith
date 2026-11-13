# A still life, annotated: one design family end to end

A 19.5 s film whose every graphic comes from one catalog family, `hw` (hand-drawn, marker on paper), so the whole piece speaks one visual language. `npx hyperframes@0.8.103 check` passes on `hyperframes@0.8.103` (0 errors, 0 warnings; 4 `container_overflow` infos are the frame's drawn border sitting outside its slot on purpose).

**Media.** This inspiration ships without its media files. In the compositions each photo or video is a labelled stand-in (its `data-media` names the file it held; `data-media-kind="video"` marks a clip), each sound a `SOUND SLOT` comment, and each font a `local()` face; the guide comment at the top of each composition says how to plug a real file back in. The files named below are the ones the film was made with. `CREDITS.md` names the source of each. `./make-footage.sh` fetches or makes them again; put each back in its slot to see the film as it was.

```bash
./make-footage.sh                      # footage/clip.mp4 (not committed)
npx hyperframes@0.8.103 add handwritten        # how the family was installed: every item tagged "handwritten", blocks AND components
```

| Time | Item | Kind | How it is wired |
|---|---|---|---|
| 0–6 | `hw-title` | block | mounted as a sub-composition over the footage; `CONFIG.text`, `scheme: "ink"`, `y: 190` edited in the file |
| 6–11 | `hw-callout-circle` ×2 | component | pasted into `compositions/notes.html`: its CSS and wrapper stack, two callouts (mug, lemon) sized around their targets, `hwCalloutBuild/On/Off` on our timeline; its family helpers live once in `compositions/hw-callout-runtime.js` |
| 10.4–12.6 | `hw-scribble-transition` | block | full coverage 0.85–1.35 s into its window, so the cut from full-frame footage to the frame sits at 10.4 + 1.1 = 11.5 |
| 11.5–19.5 | `hw-frame` | block | the frame and caption are the block; the footage is a host `<video>` placed over the block's documented slot (480, 180, 960×620), rotated −1.2° to match its tilt |

`compositions/components/hw-callout-circle.html` is the component as installed, kept as the pristine source of `notes.html`.

What "make it yours" meant for each item:
- **hw-title**: the chalk scheme failed contrast over the light wall (2.3:1, and 1.35:1 for the faint pre-sweep words), so it uses the ink scheme with the sweep off. At its default centered position the title crossed the objects, so it moved to `y: 190`.
- **hw-frame**: its stand-in fill printed the word "photo" under our footage (`text_occluded` error), so the stand-in is now empty. The block takes images only and documents a media slot, so the footage is a host `<video>` over that slot. (The block's comment that the runtime never seeks media inside sub-compositions did not reproduce on 0.8.103: a video inside a scene rendered and snapshotted frame-accurately.)
- **hw-callout-circle**: pasted whole, the composition was 530 lines (`composition_file_too_large`). Moving the shared helpers into one script file the composition loads fixed it, and is the family's own advice: copy the helpers once per host.
- `hw-title` and `hw-frame` hard-code `DUR` (6 and 8 s) instead of reading the slot, so they are mounted at exactly those durations.

Not used, though `add handwritten` installed them: `hw-path-text`, `hw-pipeline`, `hw-text-cloud`, `hw-write-title`, `hw-arrow`, `hw-boil`, `hw-box-label`, `hw-underline`, and `marker-checklist-card` (another family's item that shares the tag). Delete what a project doesn't use, and keep `hyperframes.json`'s `registryItems` to what stays.
