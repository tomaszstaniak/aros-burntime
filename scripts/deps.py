#!/usr/bin/env python3
"""Build the static SDL2, SDL2_mixer and libiconv that a mainline (ABIv1)
Burntime links, for one target, from pinned sources.

    python3 -I scripts/deps.py TARGET [--out DIR]

TARGET is aros-x86_64-abiv1-pc or aros-aarch64-abiv1-raspi. ABIv11 needs
nothing from here: its SDK ships a static SDL2 and SDL2_mixer.

Why this exists. Mainline's SDK carries SDL2 only as link stubs for the
shared SDL2.library (built from contrib), no SDL2_mixer, and the Raspberry
Pi system has no SDL2 at all. Burntime links SDL2 statically on ABIv11 and
should not depend on which SDL2.library a user's mainline system happens to
have, so both mainline targets get the static SDL2 that contrib itself
defines (`SDL2-aros-staticlib`), built here outside the AROS build system
with the same sources, patch, file list and defines.

Sources, all checked against pinned SHA-256 sums:
- SDL2 2.32.10 + contrib's `SDL2-2.32.10-aros.diff`, `SDL2_static.c` and
  the file list of `SDL2/main/mmakefile.src` (aros-development-team/contrib
  45327d2c; SDL2 last changed there in be30bc6a, which is also the current
  upstream head of that directory, checked 2026-10-28);
- SDL2_mixer 2.8.2 + contrib's `SDL2_mixer-2.8.2-aros.diff`, configured as
  contrib does but with only the decoders Burntime uses (WAV, and Ogg Vorbis
  through the bundled stb_vorbis; see MIXER_OPTS);
- GNU libiconv 1.19 + contrib's `libiconv-1.19-aros.diff`, because the AROS
  SDL2 configuration has SDL_USE_LIBICONV. The x86_64 mainline SDK already
  ships contrib's build in SDK/Extras, which is used as it is.

Compiler flags follow the mainline build's target flags (config.status of
the 2026-10-26 build: -O2, -fno-asynchronous-unwind-tables,
-fno-omit-frame-pointer, and ISA_X8664_FLAGS -m64 -mcmodel=large
-mno-red-zone -mno-ms-bitfields on x86_64). The Pi compiler wrapper adds
the Pi's own (-mcmodel=large -ffixed-x18 -fno-pic -fno-pie).

Both targets' SDL2 also gets this repository's patches in third_party/sdl2
(see its README): without them every program linking it opens gl.library
when it starts.

Output: DIR/{include,lib}, DIR/BUILD-INFO.json with every input hash and
compiler version. Default DIR: <main checkout>/deps/TARGET (outside Git).
"""
import argparse, hashlib, json, os, re, shutil, subprocess, sys, tarfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
# deps/ lives in the main checkout, shared by its worktrees; outside Git (an
# unpacked source archive) it is this directory's.
_common = subprocess.run(["git", "-C", str(REPO), "rev-parse", "--path-format=absolute",
                          "--git-common-dir"], capture_output=True, text=True)
MAIN_CHECKOUT = Path(_common.stdout.strip()).parent if _common.returncode == 0 else REPO

DIST = {
    "SDL2-2.32.10.tar.gz": "5f5993c530f084535c65a6879e9b26ad441169b3e25d789d83287040a9ca5165",
    "SDL2_mixer-2.8.2.tar.gz": "938dff531d00ace2296557a6599abe6f34599e2f34f0a4a08a397e2ccac8b8f7",
    "libiconv-1.19.tar.gz": "88dd96a8c0464eca144fc791ae60cd31cd8ee78321e67397e25fc095c4a19aa6",
}
DIST_URLS = {
    "SDL2-2.32.10.tar.gz": "https://github.com/libsdl-org/SDL/releases/download/release-2.32.10/SDL2-2.32.10.tar.gz",
    "SDL2_mixer-2.8.2.tar.gz": "https://github.com/libsdl-org/SDL_mixer/releases/download/release-2.8.2/SDL2_mixer-2.8.2.tar.gz",
    "libiconv-1.19.tar.gz": "https://ftp.gnu.org/pub/gnu/libiconv/libiconv-1.19.tar.gz",
}
CONTRIB_REV = "45327d2cee6bf27830de1ecef816804dc9259560"
CONTRIB = {
    "SDL2/main/SDL2-2.32.10-aros.diff": "44630aed7f158e7fe66dcbd15b9eb1128f2be0df5ff8ea4f1b596c17c489df41",
    "SDL2/main/SDL2_static.c": "36a888b97e664e0df2460c44a08e22896feb24e2bf29c24e1805187cdeb50648",
    "SDL2/main/SDL2_intern.h": "992b451a69ff2164d2fb895aeffb16f35b2971a6fce590680e51a42456121fd0",
    "SDL2/main/mmakefile.src": "e36eb5728e770ac4b41741fed8dd48757a3ccf15c583065a78c79feea5631936",
    "SDL2/SDL2_mixer/SDL2_mixer-2.8.2-aros.diff": "429388407b27ef2f9ce91870000bdd4e803b5959c1cea2944e75023d0d3ff837",
    "development/libs/libiconv/libiconv-1.19-aros.diff": "8511387230138865a27facc7549b354caa7273d0d53c9308eca9bb2b5b307212",
}

# Burntime decodes WAV and Ogg Vorbis only (sdlmini/src/audio.rs). Contrib's
# own options disable MP3, FLAC, shared codecs and the command player and
# leave MOD (libmikmod), MIDI and Vorbis through libvorbis on; here every
# decoder the game does not use is off, so nothing beyond the SDK is needed,
# and Ogg goes through stb_vorbis, as in the ABIv11 SDK's SDL2_mixer.
MIXER_OPTS = ["--disable-shared", "--enable-static", "--disable-music-cmd",
              "--enable-music-wave", "--enable-music-ogg", "--enable-music-ogg-stb",
              "--disable-music-ogg-vorbis", "--disable-music-ogg-tremor", "--disable-music-ogg-shared",
              "--disable-music-flac", "--disable-music-mp3", "--disable-music-mod",
              "--disable-music-midi", "--disable-music-opus", "--disable-music-wavpack",
              "--disable-music-gme", "--disable-sdltest"]


def env_path(var, default):
    return Path(os.environ.get(var, default))


def targets():
    v1 = env_path("AROS_V1_ROOT", "/Volumes/arosv1")
    pi = env_path("AROS_RASPI_TOOLCHAIN", Path.home() / "Work/AROS/toolchains/raspi-aarch64")
    sdk_v1 = v1 / "build/bin/pc-x86_64/AROS/Developer"
    return {
        "aros-x86_64-abiv1-pc": dict(
            cc=str(v1 / "toolchain-mainline/x86_64-aros-gcc"),
            ar=str(v1 / "toolchain-mainline/x86_64-aros-ar"),
            ranlib=str(v1 / "toolchain-mainline/x86_64-aros-ranlib"),
            host="x86_64-aros", sdk=sdk_v1,
            cflags=["--sysroot=" + str(sdk_v1), "-m64", "-mcmodel=large", "-mno-red-zone",
                    "-mno-ms-bitfields", "-O2", "-fno-asynchronous-unwind-tables",
                    "-fno-omit-frame-pointer"],
            iconv_prebuilt=sdk_v1 / "SDK/Extras",
            sdl2_patches=["0001-aros-call-gl.library-through-SDL-own-base.diff"]),
        "aros-aarch64-abiv1-raspi": dict(
            cc=str(pi / "bin/aarch64-aros-gcc"), ar=str(pi / "bin/aarch64-aros-ar"),
            ranlib=str(pi / "bin/aarch64-aros-ranlib"),
            host="aarch64-unknown-aros", sdk=pi / "sdk",
            cflags=["-O2", "-fno-asynchronous-unwind-tables", "-fno-omit-frame-pointer"],
            iconv_prebuilt=None,
            sdl2_patches=["0001-aros-call-gl.library-through-SDL-own-base.diff"]),
    }


def sha256(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


def run(cmd, cwd=None, log=None, env=None):
    print("+", " ".join(map(str, cmd))[-140:], flush=True)
    out = subprocess.run(list(map(str, cmd)), cwd=cwd, env=env, capture_output=True, text=True)
    if log:
        with open(log, "a") as f:
            f.write("$ " + " ".join(map(str, cmd)) + "\n" + out.stdout + out.stderr)
    if out.returncode:
        sys.stderr.write(out.stdout[-4000:] + out.stderr[-4000:])
        raise SystemExit(f"failed ({out.returncode}): {cmd[0]}")
    return out.stdout


def distfile(name, cache):
    p = cache / name
    if not p.exists():
        cache.mkdir(parents=True, exist_ok=True)
        run(["curl", "-sSL", "-o", p, DIST_URLS[name]])
    if sha256(p) != DIST[name]:
        raise SystemExit(f"{p}: SHA-256 is not the pinned one")
    return p


def contrib_file(rel, contrib):
    p = contrib / rel
    if not p.exists() or sha256(p) != CONTRIB[rel]:
        raise SystemExit(f"{p}: missing or not contrib {CONTRIB_REV[:8]}'s file")
    return p


def unpack(tgz, dest, patch, strip=1):
    top = dest / tgz.name[:-len(".tar.gz")]
    if top.exists():
        shutil.rmtree(top)
    with tarfile.open(tgz) as t:
        t.extractall(dest, filter="data")
    if patch:
        run(["patch", f"-p{strip}", "-s", "-i", patch], cwd=top)
    return top


def mm_list(text, var):
    """The file names a mmakefile variable lists (without extensions)."""
    m = re.search(rf"^{var}\s*:?=\s*\\\n((?:.*\\\n)*.*\n)", text, re.M)
    names = re.findall(r"\$\(ARCHSRCDIR\)/(\S+)", m.group(1))
    return names


def build_sdl2(t, src, contrib, out, objdir, log):
    mk = contrib_file("SDL2/main/mmakefile.src", contrib).read_text(errors="replace")
    files = mm_list(mk, "FILES") + mm_list(mk, "SDL2AROSCOREFILES") + mm_list(mk, "SDL2AROSHWFILES")
    objdir.mkdir(parents=True, exist_ok=True)
    inc = ["-I" + str(src / "include"), "-I" + str(src), "-I" + str(src / "src"),
           "-I" + str(out / "include")]
    # Contrib stamps the build date (ADATE, shown in SDL's About box); a
    # fixed one, the contrib revision's date, keeps the build reproducible.
    flags = t["cflags"] + ["-ffile-prefix-map=%s=/deps" % src.parent, "-std=gnu99", "-DSDL2_AROS_STATIC", "-D__AROS__",
                           '-DADATE="22.10.2026"'] + inc
    sources = [contrib_file("SDL2/main/SDL2_static.c", contrib)] + [src / (f + ".c") for f in files]
    shutil.copy(contrib_file("SDL2/main/SDL2_intern.h", contrib), objdir / "SDL2_intern.h")
    objs = []
    for s in sources:
        o = objdir / (str(s.relative_to(src) if s.is_relative_to(src) else s.name).replace("/", "_")[:-2] + ".o")
        run([t["cc"]] + flags + ["-I" + str(objdir), "-c", s, "-o", o], log=log)
        objs.append(o)
    # Contrib's name for its static SDL2 (`libname=SDL2_static`); `libSDL2.a`
    # in an SDK is the shared library's link stubs.
    lib = out / "lib/libSDL2_static.a"
    lib.unlink(missing_ok=True)
    run([t["ar"], "rcsD", lib] + objs, log=log)
    shutil.copytree(src / "include", out / "include/SDL2", dirs_exist_ok=True)
    return len(objs)


def build_iconv(t, src, out, objdir, log):
    objdir.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, CC=t["cc"], AR=t["ar"], RANLIB=t["ranlib"],
               CFLAGS=" ".join(t["cflags"] + ["-ffile-prefix-map=%s=/deps" % src.parent, "-std=gnu11"]))
    run([src / "configure", "--host=" + t["host"], "--disable-shared", "--enable-static",
         "--disable-nls", "--disable-rpath", "--prefix=" + str(out)], cwd=objdir, log=log, env=env)
    run(["make", "-j8", "lib/localcharset.h"], cwd=objdir, log=log, env=env)
    run(["make", "-C", "lib", "-j8"], cwd=objdir, log=log, env=env)
    shutil.copy(objdir / "lib/.libs/libiconv.a", out / "lib/libiconv.a")
    shutil.copy(objdir / "include/iconv.h", out / "include/iconv.h")


def build_mixer(t, src, out, objdir, log):
    objdir.mkdir(parents=True, exist_ok=True)
    cflags = t["cflags"] + ["-ffile-prefix-map=%s=/deps" % src.parent, "-I" + str(out / "include/SDL2"), "-I" + str(out / "include")]
    env = dict(os.environ, CC=t["cc"], AR=t["ar"], RANLIB=t["ranlib"], CFLAGS=" ".join(cflags),
               SDL_CFLAGS="-I" + str(out / "include/SDL2"),
               SDL_LIBS="-L" + str(out / "lib") + " -lSDL2_static -liconv -lGL",
               LDFLAGS="-L" + str(out / "lib"))
    run([src / "configure", "--host=" + t["host"], "--prefix=" + str(out)] + MIXER_OPTS,
        cwd=objdir, log=log, env=env)
    run(["make", "-j8", "build/libSDL2_mixer.la"], cwd=objdir, log=log, env=env)
    shutil.copy(objdir / "build/.libs/libSDL2_mixer.a", out / "lib/libSDL2_mixer.a")
    shutil.copy(src / "include/SDL_mixer.h", out / "include/SDL2/SDL_mixer.h")
    # What configure enabled: the -DMUSIC_* / -DOGG_* it put in the Makefile.
    mk = (objdir / "Makefile").read_text()
    return "\n".join(sorted(set(re.findall(r"-D(?:MUSIC|OGG|WAV|LOAD)_\w+", mk))))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("target", choices=sorted(targets()))
    ap.add_argument("--out", type=Path)
    ap.add_argument("--contrib", type=Path, default=env_path("AROS_CONTRIB", "/Volumes/arosv1/AROS/contrib"))
    a = ap.parse_args()
    t = targets()[a.target]
    out = (a.out or MAIN_CHECKOUT / "deps" / a.target).resolve()
    work = out / "work"
    if out.exists():
        shutil.rmtree(out)
    (out / "lib").mkdir(parents=True)
    (out / "include").mkdir()
    work.mkdir()
    log = out / "build.log"
    cache = MAIN_CHECKOUT / "deps/distfiles"

    info = {"target": a.target, "contrib_rev": CONTRIB_REV, "distfiles": DIST,
            "contrib_files": CONTRIB, "cflags": t["cflags"], "mixer_configure": MIXER_OPTS,
            "cc": t["cc"], "cc_version": run([t["cc"], "--version"]).splitlines()[0],
            "sdk": str(t["sdk"])}

    if t["iconv_prebuilt"]:
        for rel in ("lib/libiconv.a", "include/iconv.h"):
            shutil.copy(t["iconv_prebuilt"] / rel, out / rel)
        info["iconv"] = {"from_sdk": str(t["iconv_prebuilt"]),
                         "libiconv.a": sha256(out / "lib/libiconv.a")}
    else:
        src = unpack(distfile("libiconv-1.19.tar.gz", cache), work,
                     contrib_file("development/libs/libiconv/libiconv-1.19-aros.diff", a.contrib))
        build_iconv(t, src, out, work / "iconv-build", log)
        info["iconv"] = {"built": "libiconv 1.19 + contrib patch"}

    src = unpack(distfile("SDL2-2.32.10.tar.gz", cache), work,
                 contrib_file("SDL2/main/SDL2-2.32.10-aros.diff", a.contrib))
    # This repository's own SDL2 patches, after contrib's (third_party/sdl2).
    info["sdl2_patches"] = {}
    for name in t["sdl2_patches"]:
        p = REPO / "third_party/sdl2/patches" / name
        run(["patch", "-p1", "-s", "-i", p], cwd=src)
        info["sdl2_patches"][name] = sha256(p)
    info["sdl2_objects"] = build_sdl2(t, src, a.contrib, out, work / "sdl2-obj", log)

    src = unpack(distfile("SDL2_mixer-2.8.2.tar.gz", cache), work,
                 contrib_file("SDL2/SDL2_mixer/SDL2_mixer-2.8.2-aros.diff", a.contrib))
    info["mixer_config"] = build_mixer(t, src, out, work / "mixer-build", log).splitlines()

    info["outputs"] = {p.name: sha256(p) for p in sorted((out / "lib").glob("*.a"))}
    (out / "BUILD-INFO.json").write_text(json.dumps(info, indent=2) + "\n")
    shutil.rmtree(work)
    print(json.dumps(info["outputs"], indent=2))


if __name__ == "__main__":
    main()
