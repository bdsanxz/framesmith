# Third-party notices

Framesmith is dual-licensed MIT OR Apache-2.0. This file collects the
third-party licenses that travel with the binaries and the FFmpeg builds
the bootstrap downloads.

## FFmpeg (via ffmpeg-registry)

Framesmith never bundles FFmpeg. On first render, when the host has no
suitable `ffmpeg` on `PATH`, the binary resolves the latest
`bdsanxz/ffmpeg-registry` release for the running OS/arch programmatically,
downloads the matching asset, verifies the publisher-reported archive
digest and the binary (`-version` plus required encoders), and installs it
into the tool cache.

**The ffmpeg-registry builds are GPL-licensed.** They are built with
`libx264` (H.264 encoding) and other GPL-licensed components; the
distribution terms for that FFmpeg binary are GPL, not MIT/Apache-2.0.
Framesmith's MIT/Apache-2.0 license covers only the Framesmith code; it
does not license or replace the FFmpeg binary's GPL terms. Source for the
downloaded FFmpeg build is available from the registry release
(`https://github.com/bdsanxz/ffmpeg-registry/releases`) and from the
FFmpeg project's upstream. Using Framesmith does not create a derivative
work of FFmpeg; FFmpeg runs as a separate process, never linked or shipped
inside the Framesmith binary.

Pinned registry version at the time of this build: **9.0.2** (the first
release with x265 multilib 10/12-bit). The bootstrap resolves the latest
release at download time; if you pin an explicit build, that pin governs
instead.

## Framesmith

Copyright (c) Framesmith contributors.

Dual-licensed under:

- MIT License
- Apache License, Version 2.0

The `LICENSE` file in this package (or the repository root
`LICENSE` when built from source) carries the full text of both licenses.
The summaries below are for convenience; the full texts govern.

### MIT

Permission is hereby granted, free of charge, to any person obtaining a
copy of this software and associated documentation files (the
"Software"), to deal in the Software without restriction, including
without limitation the rights to use, copy, modify, merge, publish,
distribute, sublicense, and/or sell copies of the Software, and to permit
persons to whom the Software is furnished to do so, subject to the
following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.

### Apache-2.0

See the `LICENSE` file for the complete Apache License 2.0 terms.

## Other third-party code

The Framesmith binary links or uses the crates listed in `Cargo.lock`
(pinned at build time). Their licenses are those of the respective crates
(primarily MIT, Apache-2.0, and BSD variants). No additional GPL-licensed
Rust code is linked; GPL applies only to the separately-downloaded FFmpeg
binary as described above.
