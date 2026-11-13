# Sound shopping list — music and SFX on the beat

A 16s piece where every sound effect gets its own beat-cut moment: a terminal types the catalog search, then WHOOSH, MOUSE CLICK, KEYBOARD TYPING, IMPACT, and RISER each land on a downbeat from `audiomap.json`, and the end card slams in on the final hit. `npx hyperframes@0.8.103 check` passes on `hyperframes@0.8.103`. An SFX-only render confirmed the peaks at 2.00s (whoosh, target 2.02), 8.03s (impact, target 8.034), and 13.93s (end hit, target 14.0).

**Media.** This inspiration ships without its media files. In the compositions each photo or video is a labelled stand-in (its `data-media` names the file it held; `data-media-kind="video"` marks a clip), each sound a `SOUND SLOT` comment, and each font a `local()` face; the guide comment at the top of each composition says how to plug a real file back in. The files named below are the ones the film was made with. `CREDITS.md` names the source of each.

```bash
python3 ../../scripts/synth-drum-track.py assets/bgm.wav    # the bed; audiomap.json was analyzed from it
python3 ../../scripts/audio-peak.py assets/sfx/*.mp3          # where each sound really hits
npx hyperframes@0.8.103 check
```

Sourcing. HeyGen's catalog (`heygen audio sounds list --type sound_effects --query "…" --min-score 0.3`) needs the user's HeyGen sign-in, and the build had none, so the sounds came from Pixabay's sound-effects library (Pixabay Content License; listed in `CREDITS.md`); a new project finds its sounds with `scripts/asset-search.py audio`, and the HeyGen catalog stays an extra for a user who is signed in. The terminal scene types the real catalog command and then says plainly that the Pixabay sounds were used instead; it shows no invented search results.

Placement (every sound in `index.html` is a `SOUND SLOT` comment that names its file and time):

| Sound | Target | Measured | `data-start` |
|---|---|---|---|
| typing (keys only) | text starts typing @0.3 | onset 0.45 | 0.3 with `data-media-start="0.45"` |
| whoosh | cut @2.02 | peak 0.17 | 1.85 |
| click | press @5.039 (grid beat) | onset 0.05 | 4.989 |
| impact-bass-1 | downbeat @8.034 | peak 0.12 | 7.914 |
| riser | crest @14.0 | **peak 3.02** (the upstream description claimed the end) | 10.98, trimmed to 3.02s |
| impact-bass-1 | final hit @14.0 | peak 0.12 | 13.88 |

SFX sit under the bed (`data-volume` 0.5–0.8) and use their own audio tracks (11, 12), separate from the music (10).
