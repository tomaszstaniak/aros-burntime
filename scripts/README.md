# Scenario scripts

The scripted runs quoted in `docs/regression-recruit.md`, kept in the
repository so they outlive a scratch directory and a shared drive.

Each is a list of steps for `aros-game script <file>`: one step per line,
`'` starts a comment. They contain no game data — only clicks, keys and
waits.

**Copy them onto a guest's shared drive only while that guest is stopped.**
The drive is built when the machine starts, and changing its directory under a
running machine is changing somebody's disk while they use it.

`screens.script` walks INFO, statistics and the Saves page (see
`docs/regression-recruit.md`). Run it with `menu saves <dir> snaps <dir>` so
the slots do not land beside the default save; `snap NAME` writes the frame
to `<snaps>/NAME.png` on the host, and `type WORD` types into a name field.

`bclick.py` is not a scenario: it clicks in a pool machine's guest with the
pointer position sent in the same QMP batch as each button event, using
`vmctl.py`'s calibration and pool guard from `$AROS_TOOLS` (`python3
bclick.py [r] X Y`, with the machine's `vm.sh env` exported). `vmctl click`
sends the button alone, and on this QEMU/tablet/AROS setup such a click can
land somewhere else.

`SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy` runs any of these on the host
without a window; `snap` still writes the game's own frames.

## Running them without writing into the repository

**Use `scripts/headless.sh OUTDIR SECONDS SCRIPT [game arguments]`.** It
runs the game *in* `OUTDIR` with absolute paths to the binary and the script,
adds `snaps OUTDIR`, and writes the log to `OUTDIR/<script>.log`. Everything
the game writes relative to its working directory — `snap` pictures (which
default to `.`), the default save (`saves/game.sav`), a relative `shot`,
`trace` or `savefile` — then lands in `OUTDIR`. Game data is built into the
binary, so the working directory supplies nothing. `AROS_GAME_BIN` chooses
another binary (an older build to compare against). Without `window WxH` it
gives `window 1280x720`, the size the scripts were written at: the game
itself opens at half the desktop, as v1.2 does (UI-42), and the dummy video
driver's desktop (1024 × 768) is nobody's. B7 passes `window 640x480`, on
the host and on AROS.

Why: on 2026-10-07 a regression over every script was run with the
repository as the working directory and no `snaps`. The `snap` steps wrote
25 pictures into the repository root, four of them over tracked screenshots
(`build_asked.png`, `build_done.png`, `h1_on_ground.png`,
`h2_after_load.png`). They were restored from Git and the rest deleted; no
commit contained them, and the save digests and logs compared in that run
are unaffected. Checked afterwards: `pump.script` (one `snap`) and
`v12_modern_part.script` (default save) through `headless.sh` leave
`git status` unchanged.

`v12_modern_full`, `v12_modern_part` and `v12_modern_resume` are the Modern
opponent's save-and-resume acceptance; `v12_modern_wait` and
`v12_modern_wait_full` its save asked for while the opponents' turns are
due (`daysave`). Run them with `preset classic ai modern seed 15`
(Extended: `preset extended … start 10`); results in
`docs/audit-modern-ai.md`.

**A `*_resume` script does not end where its `*_full` does** (conflict C1,
`910a08c`): a load applies v1.2's load step — the Modern opponent's
short-term memory, the creatures' timers, a flight, the followers'
formation start afresh — and no save carries the generator. Run a resume
with `savefile <part's save>` and an explicit `seed` (the B7 suite gives it
its game's: `seed 15` above): that seeds the new process's generator, so
the run is deterministic and the same on the host and on AROS, which is
what it is checked for.

**Every script that goes through the start screen begins with a click on the
intro** (`menuclick 160,100`): the program shows v1.2's intro (`IntroScene`) on
every start before the start screen, as v0.9.1 did, and a click is how a
player leaves it (`IntroScene.cs:135-139`). A game given up comes back to the
start screen without it. `v12_intro` (`menu snaps <dir>`) photographs its
pages and animations and then skips it.

`v12_menu_start`, `v12_menu_choices`, `v12_menu_load_other` and
`v12_two_humans_{full,part,resume}` start v1.2 games from the start screen
(`menu seed 7 …`, see each header). The older menu-driven scripts were made
when the screen started the legacy game: run them with `legacy-menu`, which
keeps that (the opponent toggle's first state is "opponent on", any other
"off").

`v12_production_choice` (with `_part` and `_resume`) and `v12_info_numbers`
are B6's U1 and U2 (`docs/audit-b6.md`): the player's production choice
holding through the day, a save and a new process, and Automatic giving it
back; and INFO's food and water by v1.2's rules. Run them with `preset
classic|extended ai amiga seed 3 start 30 party give item_rat_trap` (and
`,item_hand_pump` for the numbers); the resume with `savefile <part's save>`.
Each is a prepared state (`party`, `give`) and says so. `v12_inventory_lines`
(U3) shows the pack window's v1.2 lines and `v12_rope` (U4) the rope room;
their headers have the arguments. `v12_mode` and `v12_mode_two_humans` (M1)
are the interaction mode as v1.2 keeps it: the location scene's, Talk on every
return, a handover and a load. In a v1.2 game the location menu has v1.2's
order (FIGHT or SPEAK first, then INFO): `menuline 1` is INFO there.

`press KEY` is a real key from the keyboard (KEY as `settings.txt`'s
`[keyboard]` names it). In a v1.2 game it goes to v1.2's bindings, not to
the port's test keys; `v12_keys_bound` shows it. The other steps (`day`,
`take`, …) are the script's own and never meet the bindings. `holdkey KEY MS`
holds a real key for MS milliseconds of the keys' clock (16 a frame in a
script); `v12_keys_play` and `v12_keys_handover` use it.

`v12_modern_retaliation` is the human's blow on a Modern camper and the
retaliation it starts and outlives; its header has the arguments.

`v12_service_table`, `v12_doctor_table`, `v12_doctor_cap` and
`v12_trader_set_aside` are v1.2's service tables and set-aside strip
(`ServiceScene`, `TraderScene`): a table that takes out of the pack and hands
back to the group leader first, a worthless table served, a companion far
behind who still drinks in group mode (`IgnoreRangeFilter`) and not in single
mode, the doctor's table kept across pages and the page shown treated, the
words after HEAL at the cap, and the strip lost on leaving. Their headers have
the arguments (`preset classic ai none seed 3 start 0 …`).

`v12_petko` and `v12_petko_resume` are v1.2's `/petko` dialog typed on the
map (`type /petko`, a script's typed text as the window's): each answer, the
statistics' "- Cheats active -", and the mark kept by the save; the headers
have the arguments. `v12_last_human` is a v1.2 game ending when its only
human dies (`preset classic ai amiga seed 41`): the start screen after the
death film, no machine day after it.

`resize WxH` asks the window for that client size, as a player dragging its
frame would, and the scene is laid out at the size the window gets; `f11`
switches fullscreen (the desktop's size; with the dummy driver 1024 × 768)
and back to the size the window opened at. `menuclick` and `menumove` are made
window pixels when the step is taken, in the window as it is then.
`v12_viewport` (with its control `v12_viewport_still`) resizes and switches
on the world map, in a place, over a talk dialog, in a room and on Options;
`v12_viewport_menu` resizes the start screen and clicks at and beside its
NOTES button. Both run in either look; their headers have the arguments.
`v12_viewport_intro` does the same while the intro plays and on the start
screen it leads to (a script's `f11` switches there too);
`v12_viewport_info` on INFO and the statistics and `v12_viewport_trade` at
the pub and the trader, each clicked after every change, with the controls
`v12_viewport_info_still` and `v12_viewport_trade_still` (same game lines,
same save). Each visits 600 × 449, the smallest window whose game is the
whole 320 × 200 interface (`grown_for_ui`). A pointer left where it was
covers another scene point after a resize, and in a place a person under it
stops (`Location.HoverCharacter`), so `v12_viewport_trade` moves it off the
people (`mapmove`) before walking on and onto the player's side of the narrow
trader (`scenemove`) before clicking there.
