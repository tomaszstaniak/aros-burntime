# Development

Building needs the `aros` crate of rust-aros at `../rust-aros/aros` (the release's
source archive carries it), Rust (nightly for the AROS targets), and SDL2 on the
host for the game and the viewer.

| Command | What it does |
|---|---|
| `cargo build --workspace` | host build of every crate |
| `cargo test --workspace` | host tests; most need the game's files in `burngfx/testdata` (see its README) |
| `scripts/pack.sh` | `burngfx/testdata/burntime.pack` from the files there |
| `scripts/deps.py TARGET` | static SDL2, SDL2_mixer and libiconv for an ABIv1 target, into `deps/TARGET` |
| `scripts/build.sh TARGET` | the AROS program for TARGET, into `target/TARGET/release/aros-game` |
| `scripts/package.sh TARGET VERSION REVISION OUTDIR` | the release archives for TARGET, into `OUTDIR/TARGET/` |

Targets: `aros-x86_64-abiv11-pc`, `aros-x86_64-abiv1-pc`, `aros-aarch64-abiv1-raspi`.
`targets/TARGET.env` names each target's compiler and SDK; every setting can be
overridden from the environment. Never strip an AROS executable.

Layout: game code in `aros-game/`, rules and file readers in `burngfx/`, SDL2
bindings in `sdlmini/`, the packer in `packer/`, the player's documents in
`packaging/`, build target configuration in `targets/`, the rust-aros patch series
in `third_party/rust-aros/`, the SDL2 patches for x86_64 ABIv1 in `third_party/sdl2/`, workflow commands in `scripts/` and developer tools in
`tools/`. Build output goes to `target/` and `deps/`, both ignored.
