# Outputs reference (Sprint 12, T006)

One reference project proving every Sprint 12 outcome end to end:
text over video with alpha content, delivered to all six
non-default destinations (HEVC, WebM, ProRes, GIF, PNG sequence,
HLS) at two resolution presets, with keyframe and peak-bitrate
control, an HDR (PQ) variant with tone mapping, and a provenance
stamp on every output — re-rendering byte-identically on software
paths, refusing every invalid destination before frame one, and
stopping a mid-render failure with the cause named.

## Contents

- [`composition.kdl`](composition.kdl) — the reference: root
  composition `main` (alpha video under a text mark, plus an
  audible tone) and composition `hdr` (PQ footage under a text
  mark, plus the same tone).
- Media (`clip.mov`, `tone.wav`, `hdr-pq.mp4`) is **generated**,
  not checked in: the proof script builds it with pinned host
  `ffmpeg` commands (an alpha-carrying MOV, a 2 s stereo tone,
  a 10-bit PQ HEVC source), so the reference stays diffable and
  reproducible from a clean checkout.

## The compositions

- `main` (160x120, 2 s, 15 fps, 30 frames): `clip.mov`
  (half-transparent red, composited at draw time) with
  `text value="Hi"` over it and `tone.wav` under it.
- `hdr` (same canvas and length): `hdr-pq.mp4`
  (smpte2084/BT.2020) with `text value="HDR"` over it and the
  same tone under it.

## Proof

Run [`n12-outputs-proof.sh`](../../tools/n12-outputs-proof.sh)
(one command): it builds this folder in a temp workdir, checks
green, delivers every destination twice and requires SHA-256
byte-identity, probes codec/size/provenance on each, renders two
presets and rate-controlled and HDR variants, refuses every
invalid destination before frame one, stops a vanished source
with the cause named and nothing presented as complete, and
archives versions, date, hashes, and delivered files under
`evidences/proof-n12-T006/`.
