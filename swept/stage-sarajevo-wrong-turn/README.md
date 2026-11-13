# Sarajevo, one wrong turn

A 27.5 s history short (1920×1080) showing the collage half of the `dark archival with animation` style (`references/dark-archival-with-animation.md`). The agent found every photo and sound by search and cut out the people and the pistol with local AI. It drew the map from Natural Earth data and generated the paper. Nothing was traced or prepared by hand.

**Media.** This inspiration ships without its media files. In the compositions each photo or video is a labelled stand-in (its `data-media` names the file it held; `data-media-kind="video"` marks a clip), each sound a `SOUND SLOT` comment, and each font a `local()` face; the guide comment at the top of each composition says how to plug a real file back in. The files named below are the ones the film was made with. `CREDITS.md` names the source of each. `./make-footage.sh` fetches or makes them again; put each back in its slot to see the film as it was.

```bash
./make-footage.sh                      # photos, cut-outs, prints, paper, map, music, gunshot -> assets/generated/
npx hyperframes@0.8.103 check
npx hyperframes@0.8.103 render -q draft -o renders/draft.mp4
```

The narration (`narration.txt`, `assets/narration/` (one file per line), `assets/transcript.json`) was made with `scripts/narrate.sh narration.txt bm_george .`. `hyperframes tts` failed on espeak, so the script fell back to Kokoro through misaki (`scripts/kokoro-say.py`). Names the voice did not know are written with inline phonemes in `narration.txt`.

| Scene | Starts on | Shows |
|---|---|---|
| `map.html` | "June 28, 1914" (0 s) | the map settles out of a soft focus, the canton fills, the pin drops, "SARAJEVO" types as it is spoken, the leader line reaches the pin |
| `title.html` | "wrong" (4.25 s) | the title is slapped onto olive paper; a marker underlines WRONG; a dashed route runs along the bottom and turns |
| `archduke.html` | "In the back seat" (7.16 s) | the printed cut-out whips in; the name sharpens on "Archduke"; the role types on "heir" |
| `assassins.html` | "Along his route" (12.56 s) | an orange card; "6" stamped on "six"; the trial photo as a taped print; bars across the eyes |
| `street.html` | "At the Latin Bridge" (15.06 s) | a top-down diagram: the motorcade, the planned route straight on, the car turning right, WRONG WAY! on "wrong", a ring and ENGINE STALLS on "stalls", Princip pinned on his name |
| `shots.html` | "He fires twice" (23.84 s) | the pistol; "2 SHOTS" on "twice"; two bursts on the two measured shots; a white flash and a black that holds in silence |
| `paper.html` | 0–25.4 s (off for the final black) | one paper sheet and vignette multiplied over every scene from the host |

Palette: olive sheet `#a3ab7e`, signal-orange accent `#e8702a`, khaki paper `#e4ddc6`, olive-black ink `#1b1d14` (each language version has its own; `scripts/recolor.py` moves a film between schemes).

Technique notes:
- **Crowd photo:** the trial photo is used as a whole print, because the three accused do not cut out cleanly from a crowd.
- **Paper multiply:** `#paper-slot` carries `mix-blend-mode: multiply`. Its layer stays under 0.6 opacity so the layout audit does not count it as covering the text.
- **Car tokens:** the cars are childless divs, so the turning one rotates about itself. It is marked `data-layout-allow-orbit` because it travels while it turns.
- **Sound:** the music bed stops on the first shot. Every hit is placed by its measured peak.
