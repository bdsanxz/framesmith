# Sweep — Framesmith vs HyperFrames, all inspirations (Sprint 27 T004)

- date(UTC): 2026-11-12T19:43:37Z
- machine: Darwin 27.0.0 arm64
- framesmith: framesmith 0.1.0 (bin /Users/budisantoso/Dev/bdsanxz/FramesmithWorkspace/framesmith/target/debug/framesmith)
- ffmpeg(host): ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- hyperframes: present: hyperframes 0.9.6 via npx hyperframes@0.9.6 (node v24.21.0)
- hyperframes quality flag: --quality looks (tool default, disclosed; not a tuning flag)
- inspirations root: /Users/budisantoso/Dev/bdsanxz/BeEditorWorkspace/skills/video/direcfy/inspirations
- enumerated references: 49; executed in this run: 4
- reps: 2 per engine per reference (medians decide)
- quality threshold: SSIM All >= 0.99 for 'at or above'
- workdir: /tmp/n27-sweep-subset

| reference | framesmith | hyperframes | speed | quality | verdict |
|---|---|---|---|---|---|
| keyframe-arc-sting | unrenderable E_MISSING_COMPOSITION: error[E_MISSING_COMPOSITION] /tmp/n27-sweep-subset/stage-keyframe-arc-sting/composition.kdl:1:1: cannot read composition.kdl: No such file or directory (os erro | median 11.341s | unmeasured fs=nones hf=11.341s | statistical-only [hf facts:codec_name=h264,width=1080,height=1080,duration=6.000000,] | fs: unrenderable E_MISSING_COMPOSITION: error[E_MISSING_COMPOSITION] /tmp/n27-sweep-subset/stage-keyframe-arc-sting/composition.kdl:1:1: cannot read composition.kdl: No such file or directory (os erro | hf: median 11.341s | MISS: speed=unmeasured (unmeasured fs=nones hf=11.341s); fs: unrenderable E_MISSING_COMPOSITION: error[E_MISSING_COMPOSITION] /tmp/n27-sweep-subset/stage-keyframe-arc-sting/composition.kdl:1:1: cannot read composition.kdl: No such file or directory (os erro; hf: median 11.341s |
| media-ledger-card | unrenderable E_MISSING_COMPOSITION: error[E_MISSING_COMPOSITION] /tmp/n27-sweep-subset/stage-media-ledger-card/composition.kdl:1:1: cannot read composition.kdl: No such file or directory (os error | median 11.820s | unmeasured fs=nones hf=11.820s | statistical-only [hf facts:codec_name=h264,width=1920,height=1080,duration=5.000000,] | fs: unrenderable E_MISSING_COMPOSITION: error[E_MISSING_COMPOSITION] /tmp/n27-sweep-subset/stage-media-ledger-card/composition.kdl:1:1: cannot read composition.kdl: No such file or directory (os error | hf: median 11.820s | MISS: speed=unmeasured (unmeasured fs=nones hf=11.820s); fs: unrenderable E_MISSING_COMPOSITION: error[E_MISSING_COMPOSITION] /tmp/n27-sweep-subset/stage-media-ledger-card/composition.kdl:1:1: cannot read composition.kdl: No such file or directory (os error; hf: median 11.820s |
| name-card-template | unrenderable E_MISSING_COMPOSITION: error[E_MISSING_COMPOSITION] /tmp/n27-sweep-subset/stage-name-card-template/composition.kdl:1:1: cannot read composition.kdl: No such file or directory (os erro | median 9.643s | unmeasured fs=nones hf=9.643s | statistical-only [hf facts:codec_name=h264,width=1920,height=1080,duration=5.000000,] | fs: unrenderable E_MISSING_COMPOSITION: error[E_MISSING_COMPOSITION] /tmp/n27-sweep-subset/stage-name-card-template/composition.kdl:1:1: cannot read composition.kdl: No such file or directory (os erro | hf: median 9.643s | MISS: speed=unmeasured (unmeasured fs=nones hf=9.643s); fs: unrenderable E_MISSING_COMPOSITION: error[E_MISSING_COMPOSITION] /tmp/n27-sweep-subset/stage-name-card-template/composition.kdl:1:1: cannot read composition.kdl: No such file or directory (os erro; hf: median 9.643s |
| render-endpoint | unrenderable E_MISSING_COMPOSITION: error[E_MISSING_COMPOSITION] /tmp/n27-sweep-subset/stage-render-endpoint/composition.kdl:1:1: cannot read composition.kdl: No such file or directory (os error 2 | HF-UNSOURCED missing: hyperframes.json index.html | unmeasured fs=nones hf=nones | statistical-only [no MP4 on either side] | fs: unrenderable E_MISSING_COMPOSITION: error[E_MISSING_COMPOSITION] /tmp/n27-sweep-subset/stage-render-endpoint/composition.kdl:1:1: cannot read composition.kdl: No such file or directory (os error 2 | hf: HF-UNSOURCED missing: hyperframes.json index.html | MISS: speed=unmeasured (unmeasured fs=nones hf=nones); fs: unrenderable E_MISSING_COMPOSITION: error[E_MISSING_COMPOSITION] /tmp/n27-sweep-subset/stage-render-endpoint/composition.kdl:1:1: cannot read composition.kdl: No such file or directory (os error 2; hf: HF-UNSOURCED missing: hyperframes.json index.html |

## Verdict

- executed: 4 of 49 enumerated references; passing: 0; missing: 4
- misses (named, with both deltas):  keyframe-arc-sting media-ledger-card name-card-template render-endpoint
- INCOMPLETE: 45 reference(s) not executed in this run; no pass is claimed.
