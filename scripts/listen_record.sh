#!/bin/sh
# The host recordings for the listening test (A6, docs/listening.md).
#
#   scripts/listen_record.sh OUTDIR [BIN]
#
# Plays the two listening scenarios five ways with SDL's `disk` audio
# driver, which writes exactly what the game's mixer gives (44.1 kHz, 16-bit
# stereo), and keeps for each NAME: NAME.wav, and NAME.log with every line's
# time in seconds from the start. BIN is a host build with the game data
# built in (default target/debug/aros-game). A scripted run steps once a
# frame and the dummy video driver does not wait, so the pace is the
# build's: a debug build runs about 90 frames a second, the times in
# docs/listening.md; a release build races through in seconds. The WAVs
# hold Burntime's music: keep them local, never commit or share them.
set -eu
[ $# -ge 1 ] || { echo "usage: $0 OUTDIR [BIN]" >&2; exit 2; }
repo=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$1"
out=$(cd "$1" && pwd)
bin=${2:-$repo/target/debug/aros-game}
case $bin in /*) ;; *) bin=$(pwd)/$bin ;; esac
game="preset classic ai none seed 3"

record() {
    name=$1; script=$repo/scripts/$2.script; shift 2
    mkdir -p "$out/$name.run"
    rm -f "$out/$name.raw"
    # shellcheck disable=SC2086
    (cd "$out/$name.run" && SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=disk \
        SDL_AUDIO_DISK_OUTPUT_FILE="$out/$name.raw" SDL_DISKAUDIOFILE="$out/$name.raw" \
        perl -e 'alarm 900; exec @ARGV' "$bin" $game "$@" snaps "$out/$name.run" \
        script "$script" do quit 2>&1 |
        perl -MTime::HiRes=time -ne 'BEGIN{$t0=time} printf "%7.2f %s", time-$t0, $_') > "$out/$name.log"
    python3 -I -c '
import sys, wave
data = open(sys.argv[1], "rb").read()
with wave.open(sys.argv[2], "wb") as w:
    w.setnchannels(2); w.setsampwidth(2); w.setframerate(44100)
    w.writeframes(data[:len(data) // 4 * 4])
print(f"{sys.argv[2]}: {len(data) / (44100 * 4):.1f} s")' "$out/$name.raw" "$out/$name.wav"
    rm -f "$out/$name.raw"
}

record A-list listen_map_music start 30 music remaster map_music 2
record B-keep listen_map_music start 30 music remaster map_music 1
record C-none listen_map_music start 30 music remaster map_music 0
record D-list-volume40 listen_map_music start 30 music remaster map_music 2 music_volume 40
record E-films listen_films start 0 music remaster map_music 2
(cd "$out" && shasum -a 256 ./*.wav ./*.log > SHA256SUMS)
