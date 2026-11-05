#!/bin/sh
# The release for one AROS target: a plain archive and an APKG, neither with
# any of Burntime's data in it.
#
#   scripts/package.sh TARGET VERSION REVISION OUTDIR
#
# TARGET is aros-x86_64-abiv11-pc, aros-x86_64-abiv1-pc or
# aros-aarch64-abiv1-raspi (scripts/build.sh). Builds the program for it with
# BURNTIME_EMBED=no (an empty pack is embedded; the player's own is read at
# run time), stages the drawer
#   Burntime/Burntime, Burntime/Burntime.info, Burntime/ReadMe, Manual, LICENSE,
#   LICENSE.upstream-burntime, LICENSE.libiconv, LICENSE.third-party, NOTICE, MANIFEST
# with its drawer icon beside it, and writes
#   OUTDIR/TARGET/Burntime-VERSION-arosREVISION-TARGET.zip          the plain archive
#   OUTDIR/TARGET/Burntime-VERSION-arosREVISION-TARGET.apkg.zip     the APKG (apkg-pack's)
# VERSION is the remaster's version the port follows (1.3), REVISION the
# port release (1): Burntime-1.3-aros1-TARGET.zip, the tag v1.3-aros1.
# Each target has a directory of its own, so one OUTDIR holds all three.
#
# The APKG's manifest has `arch` and `abi` and no platform field (arospkg's
# schema): an ABIv1 aarch64 package says "aarch64, v1" and nothing tells a
# client it is for the native Pi rather than Macaros. It is built so that it
# can be checked, but it must not go into a catalogue until the schema can
# say "raspi" (coding standards, "Catalogue manifests").
#
# Needs the target's toolchain (targets/TARGET.env, scripts/build.sh), the
# ABIv1 targets' scripts/deps.py libraries, and an arospkg checkout beside
# this repository for apkg-pack. Nothing is uploaded or published.
set -eu
[ $# -eq 4 ] || { sed -n '2,30p' "$0" >&2; exit 2; }
target=$1; version=$2; revision=$3; out=$4
label=$version-aros$revision
repo=$(cd "$(dirname "$0")/.." && pwd)
apkg_pack=${APKG_PACK:-$repo/../arospkg/tools/apkg-pack}
case $target in
    aros-x86_64-abiv11-pc)    arch=x86_64;  abi=v11; what="x86_64, ABIv11" ;;
    aros-x86_64-abiv1-pc)     arch=x86_64;  abi=v1;  what="x86_64, ABIv1 (mainline)" ;;
    aros-aarch64-abiv1-raspi) arch=aarch64; abi=v1;  what="aarch64, ABIv1, native Raspberry Pi" ;;
    *) echo "unknown target: $target" >&2; exit 2 ;;
esac
mkdir -p "$out/$target"
out=$(cd "$out/$target" && pwd)

# 1. The program, without data. Never stripped: a stripped AROS executable
#    loses the relocations the loader needs.
BURNTIME_EMBED=no "$repo/scripts/build.sh" "$target"
bin=$repo/target/$target/release/aros-game
# A pack's table names its entries; the program's code builds those names
# when it needs them and holds none of them whole.
if strings "$bin" | grep -q "walk/MAT_000"; then
    echo "the program carries game data: not packaging it" >&2
    exit 1
fi

# 2. The drawer.
stage=$out/stage
rm -rf "${stage:?}"
mkdir -p "$stage/Burntime"
cp "$bin" "$stage/Burntime/Burntime"
python3 -I "$repo/tools/make_icons.py" "$stage/Burntime"
cp "$repo/packaging/ReadMe" "$repo/packaging/Manual" "$repo/LICENSE" "$repo/LICENSE.upstream-burntime" "$repo/LICENSE.libiconv" "$repo/LICENSE.third-party" "$repo/NOTICE" "$stage/Burntime/"
commit=$(git -C "$repo" rev-parse HEAD)
dirty=$(git -C "$repo" status --porcelain -- aros-game burngfx sdlmini packer targets scripts/build.sh scripts/deps.py third_party/sdl2 packaging LICENSE LICENSE.upstream-burntime LICENSE.libiconv LICENSE.third-party NOTICE | head -1)
. "$repo/targets/$target.env"
{
    echo "Burntime Remastered $version for AROS, port release $revision ($what)"
    echo "target   $target"
    echo "commit   $commit${dirty:+ (with uncommitted changes)}"
    echo "rustc    $(rustc +nightly --version)"
    echo "cc       $("$AROS_GCC" --version | head -1)"
    echo "sdk      $(printf %s "$AROS_SDK" | sed "s|^$HOME/|~/|")"   # the build machine's home as ~
    case $abi in
        v1) echo "sdl2     static, scripts/deps.py: $(python3 -c 'import json,sys;d=json.load(open(sys.argv[1]));print(" ".join(k+"="+v[:16] for k,v in sorted(d["outputs"].items())))' "$(cd "$(git -C "$repo" rev-parse --path-format=absolute --git-common-dir)/.." && pwd)/deps/$target/BUILD-INFO.json")" ;;
        *)  echo "sdl2     static, the SDK's lib/libSDL2.a $(shasum -a 256 "$AROS_SDK/lib/libSDL2.a" | cut -c1-16)" ;;
    esac
    echo "built    with BURNTIME_EMBED=no: no game data"
    echo
    echo "SHA-256 of each file:"
    (cd "$stage/Burntime" && for f in Burntime Burntime.info ReadMe Manual LICENSE LICENSE.upstream-burntime LICENSE.libiconv LICENSE.third-party NOTICE; do
        printf '%s  %s\n' "$(shasum -a 256 "$f" | cut -d' ' -f1)" "$f"
    done)
} > "$stage/Burntime/MANIFEST"

# 3. The plain archive: the drawer and its icon.
plain=$out/Burntime-$label-$target.zip
rm -f "$plain"
(cd "$stage" && zip -q -X -r "$plain" Burntime Burntime.info)

# 4. The APKG: apkg-pack's description beside the drawer, then its archive.
# The C runtime's libraries: what every program built this way opens, and
# they differ by ABI: ABIv11's C library is crt.library, stdlib.library and
# m.library; ABIv1's (mainline and the Pi) is stdc.library, stdcio.library
# and posixc.library, and it has none of the ABIv11 names, so a package
# requiring those is refused there by apkg ("not on this machine"). The
# binary also names SDL2's (gl.library, lowlevel.library, …), which SDL
# opens only as it needs them; requiring those would refuse machines it
# runs on.
case $abi in
    v11) libs="crt.library stdlib.library m.library" ;;
    v1)  libs="stdc.library stdcio.library posixc.library" ;;
esac
requires=
for lib in $libs; do
    strings "$bin" | grep -qx "$lib" || { echo "the program does not name $lib: check the requirements" >&2; exit 1; }
    requires="$requires --requires $lib"
done
rm -f "$stage/Burntime.arospkg.toml"
# shellcheck disable=SC2086
python3 -I "$apkg_pack" init "$stage/Burntime" --non-interactive \
    --id burntime --version "$version" --revision "$revision" \
    --summary "Burntime Remastered 1.3 for AROS (needs your own game data)" \
    --category game/strategy --license "MIT; game data not included" \
    --kind app --arch "$arch" --abi "$abi" $requires --no-depends \
    --note "Burntime needs burntime.pack made from your own copy of the game: see ReadMe." \
    --note "Put it in a data drawer outside this one, e.g. Work:Burntime-Data, then:" \
    --note "MakeDir ENV:Burntime ENVARC:Burntime" \
    --note "SetEnv SAVE Burntime/Data Work:Burntime-Data"
apkg=$out/Burntime-$label-$target.apkg.zip
rm -f "$apkg"
python3 -I "$apkg_pack" build "$stage/Burntime" --output "$apkg"
python3 -I "$apkg_pack" check "$apkg"

ls -l "$plain" "$apkg"
