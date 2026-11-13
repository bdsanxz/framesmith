#!/usr/bin/env bash
# Regenerate this inspiration's uncommitted footage (run from the inspiration folder; SKILL_DIR = the skill root).
# assets/layers/subject.webm (remove-background of the clip's 11.5–15.5 s) and assets/stills/freeze-17s.jpg
# (the clip's frame at 17 s) are committed, because they were cut from exactly this footage.
set -euo pipefail
node "${SKILL_DIR:-../..}/scripts/make-flat-footage.mjs" footage/clip.mp4 --seconds 22 --cast none --flatness 0
