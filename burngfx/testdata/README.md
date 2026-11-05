# Test data

Empty on purpose. The files that belong here come from Burntime itself, and
Burntime is abandonware rather than freeware — its data is not ours to
redistribute, and the remaster's reference renders are derived from assets
licensed to that project alone.

Nothing here is needed to build or to test: `cargo test` runs `synthetic.rs`,
whose fixtures are assembled from the format description, and the viewer falls
back to a sprite its build script generates. Supplying these files only enables
the extra checks in `reference.rs` and makes the viewer draw real assets.

| File | Where it comes from |
|---|---|
| `MAT_090.RAW` | `resources/game/classic/` in the Burntime remaster's repository. |
| `STA.ANI` | The same directory. |
| `ref-STA/0.png` … `20.png` | `gfx/original/STA.ANI/` in that repository — the same frames, already decoded. |
| `map0-recovered.pal` | Generated: `cargo run -p burngfx --example recover_palette`. |
| `maps/MAT_000.burnmap`, `.txt`, `_tiles.png` | `resources/game/classic/maps/` in that repository. |
| `classic/…` | `resources/game/classic/` of the **v0.9.1 tag**, same relative paths, taken with `git show v0.9.1:<path>` so no checkout is switched: `gfx/ui/*`, `gfx/menu_exit*.png`, `gfx/start_maxdesign.png`, `font.*`, `highres-font.*`, `conversion_table.txt`, `lang/en/newburn.txt`, `sounds/*.ogg` — the interface's own pictures, fonts and texts. |
| `tiles/<section>_<item>.png` | `gfx/original/tiles/` there — the classic tile set, already decoded. 1219 of them; the 39 maps between them need 1122. |

With `burntime.pak` present the packer takes tiles and collision straight from
`ZEI_*.RAW` and the rendered PNGs are not used at all — which is how the six
incomplete maps became complete. What follows applies only to a pack built
without the archive.

**Not every classic tile has a rendered PNG.** The remaster ships none at all
for sections 14 and 15, and only part of 28 and 29 — those tiles live in the
player's own `zei_<section>.raw`. Six location maps are affected, and the
renderers mark the gaps rather than leaving a hole that reads as scenery:

| Map | Missing |
|---|---|
| `MAT_024` | 50 of 144 cells — most of the map, all section 14 |
| `MAT_004`, `MAT_038` | 12 each, section 29 |
| `MAT_016`, `MAT_035` | 9 each, section 29 |
| `MAT_018` | 1, section 28 |

The other 33 maps, including `MAT_000` and `MAT_005` (Nob Hill), render
completely.

| `burntime.pak` | `resources/game/` in that repository. The remaster ships the whole 1993 installation in it: `burn_gfx/gam.dat`, every `MAT_*.RAW` and `ZEI_*.RAW`, the `RAUM_*.PAC` interiors, sounds and music. Optional today — only the tests read it — but it is where the entry points, the road graph, the missing tiles and the room interiors live. |
| `lang/BURN.TXT`, `lang/newlocations.txt` | `resources/game/classic/lang/en/` there — the string tables a map's `title = burn?N` points into. **`BURN.TXT` from the v0.9.1 tag** (`git show v0.9.1:…`): later commits add a `{cheats}` marker to the statistics and fold two of its lines into one, which moves every line after it. |
| `masks/<section>_<item>.txt` | `resources/game/classic/gfx/tiles/` — ten hand-written collision masks the remaster applies *after* the ones in `ZEI_*.RAW`. Corrections, not a second opinion: nine of the ten differ and two unblock a cell. |
| `font.png`, `font.txt` | `resources/game/classic/` — the game's bitmap font and its per-character widths. |
| `items_original.txt`, `items.txt` | `resources/game/classic/` — the two item tables the game's two **modes** play by. The same files in every release. |
| `gamesettings.txt` | `resources/game/classic/` **at tag `v0.9.1`**. One file, used for both modes. |
| `gamesettings_original.txt`, `gamesettings_extended.txt` | `resources/game/classic/` **on `main`**, where that one file was split in two, one per mode. |
| `production.txt` | `resources/game/classic/` — what a camp makes, and how fast. |
| `trader.txt` | `resources/game/classic/` — what each trader restocks from, keyed by their `NameId`. |
| `ConstValues.cs` | `source/Burntime.BurnGfx/` **at tag `v0.9.1`** — **source, not a resource**, and the only entry here that is. See below. |
| `v1.2/rules/<set>/{game,items,trader,production}.txt` | `resources/game/classic/rules/<set>/` **at tag `v1.2`** (`27412d2`), for `dos`, `amiga`, `classic` and `extended` — v1.2's rule presets; Classic has only its own `game.txt` and borrows the others (see `burngfx::rules::RuleSet::files`). Remaster data (MIT), taken with `git show v1.2:<path>`. |
| `v1.2/rules/classic/ai.txt` | `resources/game/classic/ai.txt` **at tag `v1.2`** (`27412d2`) — the Modern opponent's policy, one file for every preset; `git show v1.2:resources/game/classic/ai.txt > burngfx/testdata/v1.2/rules/classic/ai.txt`, and a line for it in `v1.2/SOURCE`. Read by the packer and by `tests/modern_policy.rs::the_v12_ai_txt`, which fails, naming this file, when it is absent. |
| `v1.2/gfx/ui/start_button_amiga.png`, `start_button_amiga_down.png` | `resources/game/classic/gfx/ui/` **at tag `v1.2`** (`27412d2`): the start screen's Amiga-opponent button; `git show v1.2:resources/game/classic/gfx/ui/<file> > burngfx/testdata/v1.2/gfx/ui/<file>`, and a line in `v1.2/SOURCE`. Packed as `ui/amiga`, `ui/amiga_down`; without them the toggle shows nothing in that state. |
| `v1.2/construction.txt` | `resources/game/classic/construction.txt` **at tag `v1.2`** (`27412d2`, sha256 `4459837e…`, 1339 bytes): v1.2's recipes (the radio a `main_items` recipe, the mine detector gated by `requires_function`); packed as `v12/constructions`, which a v1.2 game builds by. The top-level `construction.txt` stays v0.9.1's, for the legacy game. |
| `v1.2/settings.txt` | `resources/game/classic/settings.txt` **at tag `v1.2`**: `[system]` with `music_volume`, `[keyboard]` and `[gamepad]` (the v1.2 defaults), and `[debug]`'s `insert_items_*` (12 items in the third). Packed as `v12/system`, which a v1.2 game and v1.2's screens read; the legacy game keeps `classic/settings.txt` (v0.9.1's) as `rules/system`. |
| `v1.2/lang/en/manual.txt`, `manualui.txt`, `setupnotes.txt` | `resources/game/classic/lang/en/` **at tag `v1.2`**: the in-game manual's text and labels, and the start screen's notes; packed as `v12/manual`, `v12/manualui`, `v12/setupnotes`, read from the player's own v1.2 files as `tooltip.txt` is. |
| `v1.2/font-small.txt`, `v1.2/font.png` | `resources/game/classic/` **at tag `v1.2`**: `font-small.txt` and the `font.png` it names (v1.2's, with the umlauts); packed as `font/small` and its strip. The manual and v1.2's Options pages draw in it. |
| `v1.2/gfx/ui/manual_city.png`, `menu_exit.png`, `menu_exit_hover.png` | `resources/game/classic/gfx/ui/` **at tag `v1.2`**: the manual's city picture (`ui/manual_city`) and v1.2's 36-pixel start-screen button behind NOTES (`ui/notes`, `ui/notes_hover`). |
| `v1.2/intro.txt` | `resources/game/classic/intro.txt` **at tag `v1.2`** (`27412d2`, sha256 `8e237c58…`, 1064 bytes; the same file at v0.9.1): the intro's pages and animations. The packer packs it as `v12/intro` with what it names — `intro/map/NN` (the tiles of `mat_080`–`084` and `089` from `burntime.pak`, of `MAT_090.RAW` from this directory), their tiles, and the `an/introN/F` frames of `intro1`–`intro4.ani`. Without it the game starts at the start screen and says so. |
| `v1.2/gfx/ui/input_glyphs_keyboard.png`, `input_glyphs_xbox.png`, `input_glyphs_playstation.png`, `input_glyphs_steam.png`, `input_glyphs_switch.png`, `input_glyphs_hold.png` | `resources/game/classic/gfx/ui/` **at tag `v1.2`** (`27412d2`): the input prompts' glyph sheets, 22-pixel cells — the keyboard and mouse (220 × 110, 10 × 5), the four controller families (88 × 88, 4 × 4, indexed by `InputGlyph - 1`), the hold mark (22 × 22). Not the `*2x.png` versions, which only xBR draws. Packed as `ui/glyphs_keyboard`, `ui/glyphs_xbox`, `ui/glyphs_playstation`, `ui/glyphs_steam`, `ui/glyphs_switch`, `ui/glyphs_hold` (`pack::names::V12_PROMPT_GLYPHS`); `tests/prompts.rs::the_packed_glyph_sheets` checks their sizes when the pack has them. |
| `v1.2/lang/en/prompts.txt`, `controls.txt`; `v1.2/classic_de/lang/de/prompts.txt`, `controls.txt` | `resources/game/classic/lang/en/` and `resources/game/classic_de/lang/de/` **at tag `v1.2`**: the prompts' labels (`@prompts?N`, 47 lines) and the names of controls drawn as text (26 lines). Packed as `v12/prompts`, `v12/controls`, `v12/de/prompts`, `v12/de/controls` (`V12_PROMPT_TEXTS`). |
| `v1.2/music/jukebox_dos.txt`, `v1.2/amiga/jukebox_amiga.txt` | `resources/game/music/jukebox_dos.txt` and `resources/game/amiga/jukebox_amiga.txt` **at tag `v1.2`**: the Jukebox's songs and their titles (`OptionsJukeboxPage`); packed as `v12/jukebox_dos`, `v12/jukebox_amiga`. |
| `v1.3/rules/<set>/*.txt`, `v1.3/lang/en/{newburn.txt,tooltip.txt,BURN.TXT,setupnotes.txt,MEN_054.TXT}`, `v1.3/classic_de/lang/de/{newburn.txt,tooltip.txt,setupnotes.txt,MEN_054.TXT}`, `v1.3/maps/MAT_038.txt`, `v1.3/SOURCE` | the same paths under `resources/game/classic/` (and `classic_de/`) **at tag `v1.3`** (`550a119`); `v1.3/SOURCE` lists them as `v1.2/SOURCE` does. The packer reads a file from `v1.3/` where there is one and from `v1.2/` otherwise (`packer::Remaster`), and packs `v1.3/SOURCE` as `v13/source`; a v1.2-preset game refuses a pack without it (docs/compliance-v1.3.md, M4). Only these files changed in v1.3 for what the port takes; v1.3's new pictures are presentation it does not take. |

**The NewGfx look's pictures** (`packer --newgfx` only; a pack without the
flag reads none of them). They are the remaster's redraws of Max Design's art
and its own new interface art, which no licence lets anyone pass on: they come
from the player's own v1.2 installation (`game/classic_newgfx/` and
`game/classic/` there; `resources/game/…` in the repository at tag `v1.2`,
`27412d2`), and nothing made from them is shipped.

| File | Where it comes from |
|---|---|
| `v1.2/newgfx/…` | `resources/game/classic_newgfx/` **at tag `v1.2`**, the whole directory, same relative paths: `newgfx.txt` (the table the packer evaluates), the `gfx/`, `maps/` and font pictures it names, `font-small-highres.txt`/`.png`, the NewGfx `font-indicators.png` and `font-icons.png`. It stands for the package v1.2 mounts over `classic` (`Application.cs:198-201`): a file here replaces one of the same name under `v1.2/`. |
| `v1.2/maps/shared/set_*.png`, `v1.2/maps/MAT_007_tiles_2x.png` (and `013`, `024`, `030`, `032`, `033`) | `resources/game/classic/maps/` **at tag `v1.2`**: the 60 × 72 tile sheets `newgfx.txt` names from the classic package. A copy of the whole `maps/` directory does no harm. |
| `v1.2/font-highres.txt`, `v1.2/font-highres.png` | `resources/game/classic/` **at tag `v1.2`**: the text font NewGfx swaps in for `font.txt` (`newgfx.txt`'s `[noscale]`). |

The packer resolves every picture the game draws through `newgfx.txt` and
packs what v1.2 would draw instead, cut to the cells used, as `ng/<classic
name>`; what v1.2 would leave classic gets no entry. A NewGfx pack is large:
53.5 MB for `minimal`, 95.6 MB for `full` (18.0 MB and 26.9 MB without),
measured 2026-10-19.

**Take the rule files from the release you mean to reproduce.** They are the
one place in this directory where the version matters and the filename does
not say so: `gamesettings.txt` exists only in v0.9.1 and the split pair only
on `main`. Reading v0.9.1's code while feeding it `main`'s numbers is a thing
that can happen quietly, and did — the two differ in start locations, respawn
rates and, at the hardest difficulty, in how much is generated. Copy them out
of a checkout of the tag you want:

```sh
git -C <burntime> show v0.9.1:resources/game/classic/gamesettings.txt > gamesettings.txt
git -C <burntime> show v0.9.1:resources/game/classic/trader.txt       > trader.txt
git -C <burntime> show v0.9.1:source/Burntime.BurnGfx/ConstValues.cs  > ConstValues.cs
```

**Why a `.cs` file is in a data directory.** What a thing is worth in a
barter is not in `items.txt`. `ItemTypesData` builds a classic item's
`TradeValue` as `ConstValues.GetValue(burngfx) / 4`, from a table that lives
in the remaster's *source*, indexed by the DOS sprite id. The packer reads
that file and converts the table into `rules/prices` in the archive, with
the original's own comments carried through.

That file is therefore both source — the remaster's, MIT, Jakob Harder —
and a table of numbers describing Max Design's game. It is here for the same
reason as everything else in this directory: it is not distributed with this
project, it is gitignored, and you are expected to have a checkout of the
remaster to take it from. Where those numbers should ultimately come from is
an open question and this arrangement does not settle it; it only keeps the
answer out of our source tree while the question stands.

`scripts/pack.sh` writes `burntime.pack` here: the maps, settings, string
tables and font, with every PNG decoded to raw RGBA. That archive is what the
AROS build compiles in, so it needs no image library and no file system. It is
as un-redistributable as the files it is made from, and is gitignored with
them.

`cargo run -p burngfx --example render_map -- MAT_000` composes a map into a PNG
next to this file, which is the quickest way to see whether a change to the map
code broke anything.

A full original installation additionally has `MAT_000.RAW`, which holds the
palette these sprites actually use. With it, `reference.rs` could compare RGBA
directly instead of reasoning about masks and palette consistency.

`construction.txt` is the remaster's own `resources/game/classic/construction.txt`
at `v0.9.1`, the same kind of file as `production.txt` and `items_original.txt`:
MIT source data from that repository, not Max Design's game data.

## `saves-aros6/` (an old save the game refuses)

| File | Source |
|---|---|
| `saves-aros6/modern-c.sav` | A save written by a test build before the first release (v1.2 Classic, Modern opponent, its pack `a90534108c237dd7`), sha256 `e7a3a645c8c71df40fb57d5790d9e4887157ea6d9abda89aa3cc5af8d7325684`. Not distributed. |

Read by `tests/save_compat.rs`: such a save is refused (`WrongPack`) with
a message. To make it again: build `4aa1bd5` with that v1.2 pack built in
and run its `modern-c-part` and `modern-c-resume` runs
(`tools/b7_suite.py host`).

