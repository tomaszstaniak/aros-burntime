#!/bin/sh
# Build the game for one AROS target.
#
#   scripts/build.sh TARGET [cargo build arguments...]
#
# TARGET is one of
#   aros-x86_64-abiv11-pc      AROS One and other ABIv11 distributions
#   aros-x86_64-abiv1-pc       mainline AROS on PC hardware
#   aros-aarch64-abiv1-raspi   native mainline AROS on the Raspberry Pi
#
# The game's code is the same for all three; what differs is the compiler,
# the SDK and where SDL2 comes from, and that is all this script chooses.
# targets/TARGET.env names the toolchain and SDK (each line can be
# overridden from the environment), targets/TARGET.json.in is the Rust
# target, written to target/specs/TARGET.json with the paths filled in, and
# the program lands in target/TARGET/release/aros-game.
#
# ABIv11 links the SDK's static SDL2 and SDL2_mixer. The ABIv1 targets link
# the static ones scripts/deps.py builds into deps/TARGET (in the main
# checkout; see that script). Toolchain, SDK and the machine that runs the
# result must be the same AROS build: nothing here checks the machine.
set -eu
[ $# -ge 1 ] || { sed -n '2,20p' "$0" >&2; exit 2; }
target=$1; shift
repo=$(cd "$(dirname "$0")/.." && pwd)
envfile=$repo/targets/$target.env
[ -f "$envfile" ] || { echo "unknown target: $target" >&2; exit 2; }

# shellcheck disable=SC1090
. "$envfile"
# deps/ lives in the main checkout, shared by its worktrees; outside Git (an
# unpacked source archive) it is this directory's.
if common=$(git -C "$repo" rev-parse --path-format=absolute --git-common-dir 2>/dev/null); then
    main_checkout=$(cd "$common/.." && pwd)
else
    main_checkout=$repo
fi
DEPS=${DEPS:-$main_checkout/deps/$target}
for p in "$AROS_GCC" "$AROS_AR" "$AROS_SDK"; do
    [ -e "$p" ] || { echo "$target: $p is missing (mounted?)" >&2; exit 1; }
done
case $target in
    *-abiv1-*) [ -f "$DEPS/lib/libSDL2_static.a" ] || {
        echo "$target: no $DEPS/lib/libSDL2_static.a; run scripts/deps.py $target" >&2; exit 1; } ;;
esac

mkdir -p "$repo/target/specs"
spec=$repo/target/specs/$target.json
sed -e "s|@CC@|$AROS_GCC|g" -e "s|@SDK@|$AROS_SDK|g" -e "s|@DEPS@|$DEPS|g" \
    "$repo/targets/$target.json.in" > "$spec"

# Source paths of this machine end up in panic locations and debug data;
# map them to fixed names. RUSTFLAGS replaces any rustflags of the cargo
# configuration (rust-aros's aros-env.toml has only such mappings).
rust_aros=$(cd "$repo/../rust-aros" 2>/dev/null && pwd || echo "$repo/../rust-aros")
RUSTFLAGS="--remap-path-prefix=$repo=/aros-burntime --remap-path-prefix=$rust_aros=/rust-aros"
RUSTFLAGS="$RUSTFLAGS --remap-path-prefix=${RUSTUP_HOME:-$HOME/.rustup}=/rustup --remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo"
export RUSTFLAGS
export AROS_GCC AROS_SDK AROS_AR BURNTIME_TARGET="$target"
cd "$repo/aros-game"
exec cargo +nightly build --release --target "$spec" "$@"
