# rust-aros, as this repository needs it

The AROS build uses a sibling checkout of
[`rust-aros`](https://github.com/tomaszstaniak/rust-aros) as a path
dependency. The published repository is not enough: files, `CreateDir` and the
directory listing the Saves page reads are in eleven commits that have not been
published there. They are kept here as a patch series, so a build does not
depend on one workstation's working copy.

| | |
|---|---|
| Base, published | `1e26a71d4b7956f358d6540163bad1def6d91360` (`main` on GitHub) |
| After the series | `4856d93172b5d45f84f2ffbef65d28fcd9c124c8`: the tree of local commit `16978a7` without its `.DS_Store` |

```sh
git clone https://github.com/tomaszstaniak/rust-aros
cd rust-aros
git checkout 1e26a71d4b7956f358d6540163bad1def6d91360
git am ../aros-burntime/third_party/rust-aros/patches/*.patch
git rev-parse HEAD^{tree}    # 4856d93172b5d45f84f2ffbef65d28fcd9c124c8
```

`git am` makes new commit ids, so the tree is what to compare, not the commit.
It was compared on 2026-09-22: the series applied to the base gives exactly
that tree.

Then, once, `setup.sh` in `rust-aros` writes the two files the build reads
(`x86_64-aros.json`, `aros-env.toml`). It finds the cross toolchain on `PATH`
or in a few fixed places; anywhere else, name it:

```sh
AROS_GCC=/path/to/x86_64-aros-gcc AROS_SDK=/path/to/sdk ./setup.sh
cd ../aros-burntime/aros-game && cargo +nightly build --release
```

**Checked on 2026-09-23 from fresh clones**: the tree matched, `setup.sh`
needed both variables, and the AROS target built, with the game archive and
without it. The binary runs on AROS One. It is not byte-identical to a build
made in another directory, because this repository's own paths are not
remapped out of it.

**One thing is left out on purpose.** Two of the local commits also add a
macOS `.DS_Store` to `rust-aros`; the series here does not carry it, so the
result is `16978a7`'s tree minus that one file and nothing else. The local
commits themselves are unchanged — correcting them belongs to `rust-aros`.

## What each patch is for

Only 2 and 4 are tooling alone — they change `tools/aros-qemu-run.py`, which
runs builds in a guest and is not part of the binary. The rest change the
`aros` crate the game links, 3 and 7 together with that tool. The series has
to apply in order either way.

| | Patch | Changes |
|---|---|---|
| 1 | exec glue compiled against the SDK it links with | `aros/build.rs`, target spec |
| 2 | `aros-qemu-run`: the disc-swap wait can be set | tooling only |
| 3 | do not say "started" when only keys were sent | `aros/src/args.rs`, tooling |
| 4 | do not claim a launch the screen cannot prove | tooling only |
| 5 | files over `dos.library` | `aros/src/fs.rs`: saves |
| 6 | `CreateDir` makes one level only | the saves directory |
| 7 | carry a file to the guest on the disc | `aros/src/fs.rs`, tooling |
| 8 | `fs::for_each_entry` over `Lock`/`Examine`/`ExNext` | the Saves page |
| 9 | no "please insert volume" requester | `pr_WindowPtr`, C glue |
| 10 | refuse a lock that is not a directory | the Saves page |
| 11 | `can_lock`: a missing directory is not a missing volume | the Saves page |

Patches 1 and 2 name their author with a personal address, as the local
commits do; the other nine use GitHub's no-reply address. That is how the
commits were made and it has been left as it is.

Publishing these to `rust-aros` itself is a separate decision and has not been
taken. When it is, this directory goes.
