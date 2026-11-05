#!/bin/sh
# Run one scenario on the host without a window, with everything it writes
# kept out of the repository.
#
#   scripts/headless.sh OUTDIR SECONDS SCRIPT [game arguments...]
#
# The game runs *in* OUTDIR (created if missing): `snap` pictures, the default
# save, a `shot` or `trace` given as a relative path, all land there. The
# binary and the script are passed as absolute paths, and `snaps OUTDIR` is
# given unless the arguments already name one. Game data is built into the
# binary; nothing is read from the working directory. The log is
# OUTDIR/<script name>.log. SECONDS is a hard limit (a hung window locked the
# screen once). Pass `savefile NAME` to choose the save; relative names are
# in OUTDIR. Without `window WxH` the window is 1280 x 720, the size every
# scenario was written at: the game itself opens at half the desktop, as
# v1.2 does, and the dummy video driver's desktop is not anyone's.
set -eu
[ $# -ge 3 ] || { echo "usage: $0 OUTDIR SECONDS SCRIPT [args...]" >&2; exit 2; }
repo=$(cd "$(dirname "$0")/.." && pwd)
out=$1; secs=$2; script=$3; shift 3
mkdir -p "$out"
out=$(cd "$out" && pwd)
case $script in /*) ;; *) script=$(pwd)/$script ;; esac
bin=${AROS_GAME_BIN:-$repo/target/debug/aros-game}
case " $* " in *" snaps "*) snaps= ;; *) snaps="snaps $out" ;; esac
case " $* " in *" window "*) window= ;; *) window="window 1280x720" ;; esac
name=$(basename "$script" .script)
cd "$out"
# shellcheck disable=SC2086
SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy perl -e 'alarm shift; exec @ARGV' "$secs" \
    "$bin" "$@" $window $snaps script "$script" do quit > "$out/$name.log" 2>&1
