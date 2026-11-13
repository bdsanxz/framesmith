#!/usr/bin/env bash
# Regenerate this inspiration's uncommitted media (run from the inspiration folder; SKILL_DIR = the skill root).
set -euo pipefail
node "${SKILL_DIR:-../..}/scripts/make-flat-footage.mjs" footage/flat.mp4 --seconds 24
