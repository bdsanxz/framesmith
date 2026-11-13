# Sarajevo, one wrong turn — Cantonese

`inspirations/sarajevo-wrong-turn/` re-voiced and re-timed in Cantonese (粤语): a 28.5 s history short (1920×1080) showing the collage half of the `dark archival with animation` style (`references/dark-archival-with-animation.md`). The photos, cut-outs, map, paper, music, and sounds are the English inspiration's. The narration, on-screen text, fonts, and every timing are new.

**Media.** This inspiration ships without its media files. In the compositions each photo or video is a labelled stand-in (its `data-media` names the file it held; `data-media-kind="video"` marks a clip), each sound a `SOUND SLOT` comment, and each font a `local()` face; the guide comment at the top of each composition says how to plug a real file back in. The files named below are the ones the film was made with. `CREDITS.md` names the source of each. `./make-footage.sh` fetches or makes them again; put each back in its slot to see the film as it was.

```bash
./make-footage.sh                      # photos, cut-outs, prints, paper, map, music, gunshot -> assets/generated/
npx hyperframes@0.8.103 check
npx hyperframes@0.8.103 render -q draft -o renders/draft.mp4
```

The narration was made with `scripts/narrate.sh narration.txt "中年男性，聲線低沉穩重，歷史紀錄片旁白" . --dialect 粤语`:
- **Voice.** VoxCPM2 designed the voice from that description. It saved the voice as `assets/voice-ref.wav` and cloned it for every line, so a re-run speaks with the same voice.
- **Script.** `narration.txt` is written in Cantonese itself (佢, 嘅, 咗, 喺, 踩掣), not standard Mandarin. The dialect only comes through when the words are the dialect's own.
- **Transcript.** `assets/transcript.json` has one unit per character, spelled as in the script (`scripts/transcribe-units.py`). Each unit is timed against the voice and corrected to the line bounds the engine measured (`scripts/snap-transcript.py`).

| Scene | Starts on | Shows |
|---|---|---|
| `map.html` | 一九一四年 (0 s) | the map settles out of a soft focus, the canton fills, the pin drops, 薩拉熱窩 types as it is spoken (2.43 s) |
| `title.html` | 轉錯 (3.81 s) | 轉錯彎 slapped onto rose paper, a marker under 錯, the route turns |
| `archduke.html` | 後座 (7.27 s) | the cut-out whips in; 斐迪南大公 sharpens on 斐迪南; the role types from 奧匈帝國, one character per syllable |
| `assassins.html` | 佢嘅路線 (12.0 s) | "6" stamped on 六; 年輕刺客 types; the trial print; bars across the eyes |
| `street.html` | 到咗拉丁橋 (15.3 s) | the car turns on 右轉; 轉錯路！ stamped on 錯; it brakes on 踩掣; ring and 引擎熄火 on 熄; 普林西普 pinned on his name |
| `shots.html` | 佢開咗兩槍 (24.85 s) | the pistol; "2 槍" on 兩; two bursts on the measured shots; flash, then black |
| `paper.html` | 0–26.4 s | the paper sheet multiplied over every scene |

Palette: dusty-rose sheet `#d6a59a`, jade accent `#2f9a86`, blush paper `#ecdcd3`, plum ink `#22141a`.

Technique notes:
- **Chinese fonts.** Anton and Special Elite have no Chinese glyphs, so their roles go to Noto Sans HK 900 (display) and Noto Serif HK 600 (typed labels). `scripts/fetch-font.mjs "Noto Sans HK" 900 --text-from compositions/*.html` downloads a font holding only the characters on screen (21 KB and 29 KB instead of megabytes). Re-run it whenever the copy changes.
- **Underline marks.** A Chinese glyph fills its em box to the bottom, so the marker sits at `bottom: 2px`, where the English inspiration uses 16px; at 16px it crosses the characters.
- **Typing rate.** One Chinese character carries a syllable, so typed labels run at 8–10 characters a second instead of 20–45 Latin letters.
- **Cuts.** Every scene starts on a sentence start from the transcript. Every reveal starts on its word's first character (frame-local = global − scene start).
