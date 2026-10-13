#!/usr/bin/env bash
# Installs the git hook of this repository for the clone it is run in: the hooks are kept in
# .githooks and git is pointed there. Run it once after cloning.
set -eu
root=$(cd "$(dirname "$0")/../.." && pwd)
git -C "$root" config core.hooksPath .githooks
echo "hooks installed: git now runs .githooks/pre-push before every push in $root"
