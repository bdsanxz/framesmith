# Projects folder — one full prompt, built

The same idea, prompted twice. `lazy.txt` ("make an animation of a computer folder opening") gets a generic guess that freezes after a few seconds. `prompt.txt` makes all six decisions, and this project is that prompt built faithfully. `npx hyperframes@0.8.103 check` passes on `hyperframes@0.8.103`.

**Media.** This inspiration uses no media files: every picture is drawn in HTML, CSS, or SVG, it has no sound, and its font (Inter) is among the families the renderer bundles, so it renders as it is.
```bash
node ../../scripts/prompt-check.mjs lazy.txt     # spec and beats missing → exit 1
node ../../scripts/prompt-check.mjs prompt.txt   # passes; one info: "no narration" still allows music
```

| Decision | In `prompt.txt` | In the build |
|---|---|---|
| Deliverable | "Motion graphic." | a short unnarrated piece |
| Spec | 6-second, 1080×1080 | `data-duration="6"`, 1080×1080 canvas |
| Beats | 0–2s glide, 2–3s double-click and open, 3–6s fan-out | three time ranges on one timeline (`compositions/desktop.html`) |
| Copy | "Projects" | the Finder-style label, selected on the first click |
| Technique | "faithful Apple style", "soft squash-and-settle", "soft shadow" | lid hinges in 3D from its bottom edge; body squashes 0.93 then settles with an elastic ease; each document casts a blurred shadow |
| Negatives | no narration, no image or media files | no audio elements, every visual is HTML/CSS/SVG |

The tail is specified ("settles into a slow, subtle drift"), so the last two seconds carry motion instead of a frozen hold. Each beat is a range on one scene's timeline rather than its own sub-composition, because the folder is one continuous object across all three beats.
