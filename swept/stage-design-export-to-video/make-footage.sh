#!/usr/bin/env bash
# Regenerate this inspiration's uncommitted media (run from the inspiration folder): assets/bed.mp4,
# a 12 s slow gradient loop in the Lumen palette (ground #0B1020 into deep blue and a dark mint).
set -euo pipefail
mkdir -p assets
ffmpeg -v error -y -f lavfi \
  -i "gradients=s=1920x1080:d=12:r=30:speed=0.008:c0=0x0b1020:c1=0x1c2550:c2=0x0f3a33:nb_colors=3" \
  -c:v libx264 -pix_fmt yuv420p -g 30 assets/bed.mp4
