# SDL2 patches for the ABIv1 builds

`scripts/deps.py` builds the static SDL2 that the ABIv1 targets link from
SDL2 2.32.10 and contrib's `SDL2-2.32.10-aros.diff` (contrib `45327d2c`).
For both ABIv1 targets it then applies the patches here, in order, with
`patch -p1` in the SDL2 source tree. `BUILD-INFO.json` records each
patch's SHA-256.

| Patch | Status |
|---|---|
| `0001-aros-call-gl.library-through-SDL-own-base.diff` | candidate: https://github.com/aros-development-team/contrib/pull/87 (open) |

## 0001: call gl.library through SDL's own base

Contrib's `SDL_arosopengl.c` opens gl.library itself and keeps the base in
its driver data (`GLBase` is defined as that field), but it declares the
`glA*` functions from `<GL/gla.h>` without `<proto/gl.h>`. The calls
therefore go to libGL's link stubs, and those stubs bring libGL's
auto-open: every program that links this static SDL2 opens gl.library when
it starts, whether or not it ever asks SDL for OpenGL.

On mainline that matters. The first program of a boot that opens
gl.library leaves its output stream to the later programs of its Shell, and
with a redirected output it ends in a crash
([AROS issue 2100](https://github.com/aros-development-team/AROS/issues/2100)).

The patch includes `<proto/gl.h>` with `__INLINE_GL_STACKCALL__`, so the
calls go through SDL's own base. `inline/gl.h` has no inlines for the two
functions that take a tag list, `glACreateContext` and `glASetRast`; the
patch calls their vectors (LVOs 5 and 11 of `gl.conf`) in the same way as
the generated inlines call the others. The library then references no
libGL symbol, and Burntime's ABIv1 builds no longer link `-lGL`.
Burntime also asks SDL for its software renderer by name on every AROS
target (`sdlmini`, `sdlmini_no_gl`), so SDL never loads gl.library at run
time either. Before this change the renderer was the software one as well.
The ABIv11 build keeps the SDK's SDL2 and gives its six `glA*` references
stand-ins that fail (`sdlmini/src/gl_standins.rs`).

Tested 2026-11-04 on x86_64 mainline (QEMU, softpipe): an SDL program
creating a GL window, then five times a context, make-current, 30 clears
and swaps and delete, then closing the window and quitting, works with the
patched static SDL2 and with the in-tree `SDL2.library` built from the
contrib branch; gl.library is loaded only when the GL window is created.
