# What the 1993 formats turned out to be, and how far the tests go

Moved out of the README.

## What the formats turned out to be

Written down because it took reading the C# to find out, and none of it is
documented anywhere else.

**Characters are sixteen frames each, in one sheet.** `syssze.raw` is the same
RLE-over-mode-X format as the item pictures and takes the same default palette
(`map0`, out of `MAT_000.RAW`), and a character's body is
`syssze.raw?<SpriteId>-<SpriteId+15>` — the record's own `SpriteId` and the
fifteen frames after it. The classic file holds 214 frames of 16x16: thirteen
bodies of sixteen, then six left over.

| frames | what |
|---|---|
| 0 | standing |
| 2-3 | walking right |
| 4-5 | left |
| 6-7 | down |
| 8-9 | up |
| 12-15 | dying, and then gone |
| 208-213 | the spark drawn on both fighters when a blow lands |

**Vertical wins over horizontal**: `Character.Update` tests `dir.y` first, and
the magnitude comparison that would have picked the larger axis is commented
out in the source. A step that moves at all in `y` is drawn facing up or down.
A body stands *on* its position — `Rect(Position.x - Width / 2, Position.y -
Height, …)` — while the spark sits centred on it. Walking runs at ten frames a
second (`ani.Speed = 10`), dying at the default five, the spark at twenty.

**Palettes live in map files.** `MAT_000.RAW` … `MAT_037.RAW` are loaded by the
game purely so their palettes can be registered as `map0` … `map37`, and `map0`
is the default palette for every asset that has no entry of its own. The palette
sits at offset 16: 256 RGB triples in 6-bit VGA range, scaled by 4 on load, with
index 0 always transparent.

**Sprites are RLE over VGA mode-X.** After a `u16` width and height come pairs of
bytes: `(skip, count)`. `count == 0xFF` ends the current pass, and there are four
passes — one per mode-X plane — each starting at pixel offset `pass`. Otherwise
the write cursor advances by `4 * skip` and `count` palette indices follow.

**The write cursor addresses a 320-pixel-wide screen, not the sprite.** A 32-wide
sprite stores 8 pixels per pass per line, then a skip of `(320-32)/4 = 72` to
reach the next line. This is why `x` and `y` come from one counter divided by
320, and it is the single detail that makes the format look wrong until it
suddenly looks right.

**Frame tables are self-delimiting.** A sprite file opens with little-endian
`u16` file offsets; the first entry points at the first frame, so it also says
how long the table is. The table ends at the first zero entry or the first
offset past end-of-file. A frame runs from its offset to *one byte before* the
next — the original's `GetSubFile(start, next - 1)`, faithfully reproduced.

## How far the tests actually go

`tests/synthetic.rs` needs no game data: its fixtures are assembled here from the
format description above, and they pin down each rule separately — the four
passes, the 320-wide cursor, the offset table, out-of-bounds pixels, rejection of
implausible sizes. This is what CI can run.

`tests/reference.rs` is the stronger evidence and the weaker guarantee. It needs
`STA.ANI` plus the remaster's own renders of the same 21 frames, and skips itself
when they are absent.

- **Transparency is checked exactly.** Index 0 and nothing else is transparent,
  whatever the palette, so comparing masks is a real per-pixel check of where
  every pixel landed. All 21 frames match.
- **Colours are checked for consistency, which is not the same as correctness.**
  `STA.ANI`'s palette lives in `MAT_000.RAW`, which only an original installation
  has, so there is nothing to compare RGBA against. Instead, decoded index plus
  reference colour reconstructs a palette: 48 entries across roughly 13,000
  opaque pixels, every one a legal 6-bit VGA colour. A misplaced pixel whose
  index differed from its neighbour's would normally produce a colour conflict;
  swapping two pixels that share an index would not be caught.

Together that is strong evidence the decoder is right, not proof. Proof needs an
RGBA comparison against a real `MAT_000.RAW`.

Two limits worth stating plainly:

- `recover_palette.rs` recovers only the entries `STA.ANI` actually uses — 48 of
  256. The rest stay black. The viewer therefore shows correct colours *for this
  animation*, not a restored `map0`.
- The decoder is faithful to the original, which means it is permissive: a
  truncated frame yields a partial image rather than an error, and nothing checks
  that all four pass terminators arrived. That suits the game's own files. It is
  not yet a parser to point at untrusted input.
