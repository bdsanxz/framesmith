#!/usr/bin/env bash
# Regenerate this inspiration's uncommitted footage (run from the inspiration folder; SKILL_DIR = the skill root).
set -euo pipefail
node "${SKILL_DIR:-../..}/scripts/make-flat-footage.mjs" footage/clip.mp4 --seconds 20 --cast none --flatness 0
