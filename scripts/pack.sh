#!/bin/sh
# The developer's pack: packer over the repository's own burngfx/testdata,
# written back there for aros-game's build to embed.
#
#   scripts/pack.sh [PROFILE | MAT_NNN ...]
#
# A user makes a pack from their own files with `packer --from DIR --out FILE`
# (docs/distribution.md); nothing here is a path the program knows.
set -eu
repo=$(cd "$(dirname "$0")/.." && pwd)
exec cargo run --manifest-path "$repo/Cargo.toml" -q -p packer -- \
    --from "$repo/burngfx/testdata" --out "$repo/burngfx/testdata/burntime.pack" "$@"
