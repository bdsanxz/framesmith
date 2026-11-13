# Media ledger card — every asset resolved, nothing redrawn

A 5s card whose every non-HTML asset came through `npx hyperframes@0.8.103 media-use resolve` and lives in the project's `.media/` ledger. `npx hyperframes@0.8.103 check` passes on `hyperframes@0.8.103`. Built with no HeyGen sign-in, using only the routes that work without an account.

**Media.** This inspiration ships without its media files. In the compositions each photo or video is a labelled stand-in (its `data-media` names the file it held; `data-media-kind="video"` marks a clip), each sound a `SOUND SLOT` comment, and each font a `local()` face; the guide comment at the top of each composition says how to plug a real file back in. The files named below are the ones the film was made with. `CREDITS.md` names the source of each.

```bash
npx hyperframes@0.8.103 media-use resolve --doctor
npx hyperframes@0.8.103 media-use resolve --type logo --entity github --intent "GitHub logo" --project .        # official mark, no account
npx hyperframes@0.8.103 media-use resolve --type sfx --intent "whoosh" --project .                              # the CLI's own kit
npx hyperframes@0.8.103 media-use resolve --type sfx --intent "click" --project .                               # the CLI's own kit
npx hyperframes@0.8.103 media-use resolve --type bgm --intent "drum stand-in bed, 5s" --from bed-5s.mp3 --project .   # ingest a local file
```

`.media/index.md` is the inventory the agent reads instead of searching again:

```text
id        type   dur   path                          description
sfx_001   sfx    0.6s  .media/audio/sfx/sfx_001.mp3  Punchy whoosh/impact — fast reveal or hard transition accent.
logo_001  logo   —     .media/images/logo_001.svg    GitHub logo (official mark)
sfx_002   sfx    0.4s  .media/audio/sfx/sfx_002.mp3  Crisp UI click — button press, toggle, selection.
bgm_001   bgm    —     .media/audio/bgm/bgm_001.mp3  bed-5s.mp3
```

Findings from building it:
- Without a sign-in, `bgm`, `image`, `voice`, and any SFX outside the CLI's 19 built-in ones ("camera shutter") fail with `no provider could resolve`. Logos, the CLI's built-in SFX, `grade`, `lut`, `--from` ingest, and `--adopt` work.
- A resolved official mark arrives in its own color. GitHub's is near-black and vanished on the dark ground. `check` passed anyway (images aren't contrast-audited); the snapshot showed it. The fix is the brand's light variant: `filter: invert(1)` on a monochrome mark. Never recolor a multi-color mark.
- SFX placement still uses measured peaks and onsets (whoosh at 0.33s so its 0.17s peak lands at 0.5s; click 0.05s early).
- Each resolve is also promoted to the global cache `~/.media/`, so the next project can reuse it (`--candidates`, then `--reuse <sha>`).
