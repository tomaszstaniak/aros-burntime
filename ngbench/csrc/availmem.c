/* exec's AvailMem and a file's size, for the NewGfx bench.
 *
 * Through proto/exec.h, as rust-aros' execglue.c reaches exec: no exec stub
 * symbol is referenced, so libexec.a's member with its own `close` is never
 * pulled in (see rust-aros/aros/csrc/execglue.c). Compiled with the same
 * AROS_GCC and AROS_SDK as that file, because the inline call through
 * SysBase differs between AROS builds.
 */
#include <proto/exec.h>
#include <exec/memory.h>
#include <stdio.h>

/* Free memory of any kind, in bytes, and the largest single block. */
unsigned long ngbench_avail_any(void)
{
    return AvailMem(MEMF_ANY);
}

unsigned long ngbench_avail_largest(void)
{
    return AvailMem(MEMF_ANY | MEMF_LARGEST);
}

/* The size of a file, or -1: so the pack is read into a buffer of exactly
 * its size, and the memory reading is the pack's, not a guess's. */
long ngbench_file_size(const char *path)
{
    FILE *f = fopen(path, "rb");
    long n;
    if (!f)
        return -1;
    if (fseek(f, 0, SEEK_END) != 0) {
        fclose(f);
        return -1;
    }
    n = ftell(f);
    fclose(f);
    return n;
}
