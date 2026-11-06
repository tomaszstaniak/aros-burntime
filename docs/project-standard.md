# Project standard and adaptations

## Adoption

This repository follows the AROS project layout standard `project-standards`
0.5-proposal, structure version 3, profile `aros`, in `pilot` mode. A later
version of the standard needs a new decision.

## Configuration

- Targets: `aros-x86_64-abiv11-pc`, `aros-x86_64-abiv1-pc`,
  `aros-aarch64-abiv1-raspi` (`targets/`). The Raspberry Pi gets a plain
  archive only, no APKG.
- Release archives: `Burntime-<version>-<system>-<arch>-<abi>-<platform>.zip`,
  `Burntime-<version>-source.zip` and one `SHA256SUMS`; the version is
  `1.3-aros1` (the remaster's version, then the port release).
- External sources this repository changes are kept as patches with their
  base and status: `third_party/rust-aros/` and `third_party/sdl2/`.
- Build dependencies outside the repository: the `aros` crate of
  [rust-aros](https://github.com/tomaszstaniak/rust-aros) at `../rust-aros`
  (the source archive contains it), and for packaging `apkg-pack` from an
  [arospkg](https://github.com/tomaszstaniak/arospkg) checkout at
  `../arospkg` (or `APKG_PACK`). `targets/*.env` give the author's toolchain
  locations as defaults; each can be overridden from the environment.

## Adaptations

1. **No AROS target opens gl.library.** The remaster draws through
   MonoGame/OpenGL; this port draws every pixel into its own buffer and SDL2
   copies it to the window with its software renderer. The ABIv1 builds
   (x86_64 and Raspberry Pi) patch SDL2 so that it calls gl.library only
   through the base it opens itself (`third_party/sdl2/`); the ABIv11 build
   keeps the SDK's SDL2 and gives its six `glA*` references stand-ins that
   fail (`sdlmini/src/gl_standins.rs`). Every target names the software
   renderer. Reason: libGL's link stubs opened gl.library when the program
   started; gl.library then kept the program's drawer in use until a
   restart, and on AROS mainline the first program that opens it leaves its
   output stream to later programs of its Shell
   ([AROS issue 2100](https://github.com/aros-development-team/AROS/issues/2100)).
