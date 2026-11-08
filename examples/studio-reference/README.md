# Studio reference project (Sprint 16, T009)

One reference Studio project exercising every sprint behavior end to end:

- shell: dockable preview + timeline panels, per-project layout, light/dark theme
- timeline tools: `title` (0..2s) and `bye` (2..4s) text clips for select, razor,
  trim, split-at-playhead, and work-area gestures, each landing as a named
  history turn through the shared SDK undo system
- keyframes: `x` (three stops, one eased) and `opacity` (two stops) lanes on the
  first clip, with out-of-segment drags clamping to the segment
- beats: the strip starts empty; the proof adds, moves, deletes, and undoes
- clip visuals: `s.wav` (sounding audio with gain and fades) shows a waveform
  with peak marks; `v.mp4` (graded video) shows a filmstrip; the texts show
  their content; the fade transition bridges both texts; deleting the media
  mid-session shows named placeholder boxes (REC-005)
- shortcuts: project remap rebinds one key with the 22-action default map
  intact underneath
- conflicts: an external disk rewrite under paused unsaved work banners disk
  versus unsaved; the maker's pick applies with the losing version kept

Media (`s.wav`, `v.mp4`) is generated into the staged copy by
`tools/n16-studio-proof.sh` (Python sine WAV + ffmpeg testsrc), so the
checkout carries only this KDL file and this README. The proof never writes
into the checkout: it stages this directory to a workdir first.
