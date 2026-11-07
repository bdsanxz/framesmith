# player-sdk reference (Sprint 15, T009)

One reference project proving the player and edit SDKs hold together
end to end. Exercised by `tools/n15-player-sdk-proof.sh`, which stages
a copy (the checkout is never written) and gates:

- embedded playback with ordered player events (`ready`, `state`,
  `timeline`, `assets-ready`, `painted`, then `playback`);
- a headless edit round-trip through exact undo and redo;
- variable declare, validate, and usage scan;
- timeline plan counts with apply receipts and verb undo;
- guarded interleaved history turns across named writers.

Composition `player-sdk` (320x180, 6 s, 10 fps): three text clips
(`a`/`b`/`c`) with CSS-like hyphenated attributes (`font-family`
try-order preserved end to end). Variables `title`, `accent`, `show`,
`step`, and `brand` are declared on disk and referenced through
`var(--…)` so the scan finds every use with file, line, and column.
