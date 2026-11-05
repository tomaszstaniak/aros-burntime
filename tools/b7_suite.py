#!/usr/bin/env python3
"""B7's scripted acceptance: one list of runs, played on AROS and on the host.

    tools/b7_suite.py aros PREFIX PROGRAM [ID...] > run
                                                  an AROS batch: each run's log
                                                  to RESULTS:PREFIX-<id>.log
                                                  (only the runs ID..., if given;
                                                  B7_ROOT, default SYS:bt, is the
                                                  guest drawer with data/ and
                                                  scripts/)
    tools/b7_suite.py host OUTDIR [BIN [ID...]]   the same runs on the host
    tools/b7_suite.py compare HOSTDIR AROSDIR PREFIX
                                                  run by run: the logs and the
                                                  saves the logs name

Runs are played in order: a part writes the save its resume loads, in a new
process told the file (`savefile`) and a `seed`, and the straight run of the
same game writes its own. Both play in a 640 x 480 window (`window
640x480`; the game itself opens at half the desktop, as v1.2 does), so what
depends on the view (a camera's pan) is the same. On AROS the saves go to
the data drawer's `Saves/b7/`.

**A resume is not its straight run** (conflict C1, the owner's decision):
the save holds what v1.2's holds, and a load applies v1.2's load step — the
Modern opponent's short-term memory, the creatures' timers, a flight, the
followers' formation are gone, and no save carries the generator. So the
resume's `seed` seeds the new process's generator: that is the controlled
randomness that makes the run deterministic, and the same on the host and
on AROS. What a resume run shows is that it is deterministic and that the
host equals AROS, not that it equals its straight run.

Nothing here judges a result: `compare` says which runs differ and how; the
acceptance (docs/release-readiness.md) says what they show.
"""
import os
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# The seeds are chosen so that the game generator's draw starts the human
# where each script plays (Classic: Stove Vent; Extended: Sherwood), and so
# that the script's clicks reach what they aim at. Since a v1.2 game draws
# everything from one generator (.NET's Random), the scripts' first seeds
# (7 and 5) start him elsewhere; and the people wandering about a place are
# drawn too, so one standing in a door can take a click meant for it.
MODERN = "ai modern seed 15"
AMIGA7 = "preset classic ai amiga seed 86"
# Extended/Amiga from seed 7, not 3 (aros8): under 3 v1.3's New Village moved
# the world's draw and the resume ended in the human's death on day 13
# instead of a save on day 19; 7 starts him at Sherwood and plays the long
# resume to day 19 again (part 113 lines, resume 145, full 213).
AMIGA5 = "preset extended ai amiga seed 7"

# Each resume run's seed, for its new process's generator (see above): the
# seed its game was started with, 1993 (the default) where that game named
# none.
#
# (id, script, arguments, save): `save` names the save file a run shares
# with the others of its game; None leaves the game's default.
RUNS = [
    # Both presets, the Amiga opponent: straight, cut, resumed in a new process.
    ("amiga-c-full", "v12_amiga_full", AMIGA7, "amiga-c-full"),
    ("amiga-c-part", "v12_amiga_part", AMIGA7, "amiga-c"),
    ("amiga-c-resume", "v12_amiga_resume", "seed 86", "amiga-c"),
    ("amiga-e-full", "v12_amiga_extended_full", AMIGA5, "amiga-e-full"),
    ("amiga-e-part", "v12_amiga_extended_part", AMIGA5, "amiga-e"),
    ("amiga-e-resume", "v12_amiga_extended_resume", "seed 7", "amiga-e"),
    # Both presets, the Modern opponent; a resume is told the file and the
    # seed.
    ("modern-c-full", "v12_modern_full", "preset classic " + MODERN, "modern-c-full"),
    ("modern-c-part", "v12_modern_part", "preset classic " + MODERN, "modern-c"),
    ("modern-c-resume", "v12_modern_resume", "seed 15", "modern-c"),
    ("modern-e-full", "v12_modern_full", "preset extended start 10 " + MODERN, "modern-e-full"),
    ("modern-e-part", "v12_modern_part", "preset extended start 10 " + MODERN, "modern-e"),
    ("modern-e-resume", "v12_modern_resume", "seed 15", "modern-e"),
    ("modern-c-in-full", "v12_modern_in_full", "preset classic " + MODERN, "modern-c-in-full"),
    ("modern-c-in-part", "v12_modern_in_part", "preset classic " + MODERN, "modern-c-in"),
    ("modern-c-in-resume", "v12_modern_in_resume", "seed 15", "modern-c-in"),
    ("modern-e-in-full", "v12_modern_in_full", "preset extended start 10 " + MODERN, "modern-e-in-full"),
    ("modern-e-in-part", "v12_modern_in_part", "preset extended start 10 " + MODERN, "modern-e-in"),
    ("modern-e-in-resume", "v12_modern_in_resume", "seed 15", "modern-e-in"),
    ("modern-c-wait-full", "v12_modern_wait_full", "preset classic " + MODERN, "modern-c-wait-full"),
    ("modern-c-wait", "v12_modern_wait", "preset classic " + MODERN, "modern-c-wait"),
    ("modern-e-wait-full", "v12_modern_wait_full", "preset extended start 10 " + MODERN, "modern-e-wait-full"),
    ("modern-e-wait", "v12_modern_wait", "preset extended start 10 " + MODERN, "modern-e-wait"),
    ("modern-c-fight-gas", "v12_modern_fight", "preset classic " + MODERN + " modern-fight gas", None),
    ("modern-c-fight-clear", "v12_modern_fight", "preset classic " + MODERN + " modern-fight clear", None),
    ("modern-e-fight-gas", "v12_modern_fight", "preset extended " + MODERN + " modern-fight gas", None),
    ("modern-e-fight-clear", "v12_modern_fight", "preset extended " + MODERN + " modern-fight clear", None),
    # The human's blow on a Modern camper and the retaliation it starts.
    ("modern-retaliation", "v12_modern_retaliation",
     "preset classic ai modern seed 37 start 27 give item_full_wineskin,item_meat", "retaliation"),
    # The start screen as it opens (Extended, Modern): straight, cut, resumed
    # in a new process; and its choices changed (Classic, one step on: Amiga).
    ("menu-full", "v12_menu_full", "menu seed 7", "menu-full"),
    ("menu-part", "v12_menu_part", "menu seed 7", "menu"),
    ("menu-resume", "v12_menu_resume", "seed 7", "menu"),
    ("menu-choices", "v12_menu_choices", "menu seed 1", "menu-choices"),
    # Two humans, from the start screen (and so with Modern opponents).
    ("two-full", "v12_two_humans_full", "menu seed 7 start 0 start2 1", "two-full"),
    ("two-part", "v12_two_humans_part", "menu seed 7 start 0 start2 1", "two"),
    ("two-resume", "v12_two_humans_resume", "seed 7", "two"),
    # Combat, both presets: table blows and encounters with the dogs.
    ("fight-c-full", "v12_fight_full", "preset classic ai none start 3", "fight-c-full"),
    ("fight-c-part", "v12_fight_part", "preset classic ai none start 3", "fight-c"),
    ("fight-c-resume", "v12_fight_resume", "seed 1993", "fight-c"),
    ("fight-e-full", "v12_fight_full", "preset extended ai none start 3", "fight-e-full"),
    ("fight-e-part", "v12_fight_part", "preset extended ai none start 3", "fight-e"),
    ("fight-e-resume", "v12_fight_resume", "seed 1993", "fight-e"),
    # The endings: a player's death, the last rival's victory declined and taken.
    ("victory-full", "v12_victory_full", "preset classic ai none seed 36 players A,B", "victory-full"),
    ("victory-part", "v12_victory_part", "preset classic ai none seed 36 players A,B", "victory"),
    ("victory-resume", "v12_victory_resume", "seed 36", "victory"),
    ("victory-accept", "v12_victory_accept", "preset classic ai none seed 36 players A,B", "victory-accept"),
    # Death and respawn of people, both presets.
    ("respawn-c-full", "v12_respawn_classic_full", "preset classic ai amiga seed 11", "respawn-c-full"),
    ("respawn-c-part", "v12_respawn_classic_part", "preset classic ai amiga seed 11", "respawn-c"),
    ("respawn-c-resume", "v12_respawn_classic_resume", "seed 11", "respawn-c"),
    # 164, not 170: v1.3's New Village is one more place for the world's
    # draw, and under 170 the second hire and the spring walk miss (aros7).
    ("respawn-e-full", "v12_respawn_extended_full", "preset extended ai amiga seed 164", "respawn-e-full"),
    ("respawn-e-part", "v12_respawn_extended_part", "preset extended ai amiga seed 164", "respawn-e"),
    ("respawn-e-resume", "v12_respawn_extended_resume", "seed 164", "respawn-e"),
    # Controls and the rooms, both presets where the scenario says so.
    ("keys-walk", "v12_keys_walk", "preset classic ai none seed 3 start 30", "keys-walk"),
    ("options-keys", "v12_options_keys", "preset classic ai none seed 3 start 30", "options-keys"),
    ("sticks", "v12_sticks", "preset classic ai none seed 3 start 30", "sticks"),
    ("settings-keys", "v12_settings_keys", "preset classic ai none seed 3 start 30 music remaster", None),
    ("room-films-c", "v12_room_films", "preset classic ai none seed 3 start 0", None),
    ("room-films-e", "v12_room_films", "preset extended ai none seed 3 start 0", None),
    ("scene-doors", "v12_scene_doors", "preset classic ai none seed 3 start 8", None),
    ("church", "v12_church", "preset extended ai none seed 3 start 12", None),
    ("hover-bars", "v12_hover_bars", "preset classic ai none seed 3 start 30 party give item_rat_trap", None),
    ("music-list", "v12_music_list", "preset classic ai none seed 3 start 30 music remaster map_music 2", None),
]


def args_of(run, save_path):
    words = run[2].split()
    if run[3] is not None:
        words += ["savefile", save_path(run[3])]
    return words


# AROS's Shell cuts a command line at 255 characters (measured: a typed run
# stopped mid-word at exactly that length), and a cut line is a different run
# that may still pass.
AROS_LINE = 255


def aros(prefix, program, only=()):
    root = os.environ.get("B7_ROOT", "SYS:bt")
    runs = [run for run in RUNS if not only or run[0] in only]
    if only and len(runs) != len(set(only)):
        sys.exit(f"unknown run ids: {sorted(set(only) - {run[0] for run in runs})}")
    saves = f"{root}/data/Saves/b7"
    longest = max(len(f"{program} >RESULTS:{prefix}-{run[0]}.log window 640x480 "
                      f"{' '.join(args_of(run, lambda s: f'{saves}/{s}.sav'))} "
                      f"script {root}/scripts/{run[1]}.script do quit") for run in runs)
    if longest > AROS_LINE:
        sys.exit(f"a command would be {longest} characters, over AROS's {AROS_LINE}: "
                 f"give a shorter PROGRAM (its drawer on the Path, `Path <drawer> ADD`, and its name alone)")
    print(f"; burntime-rs B7 scripted acceptance ({len(runs)} of {len(RUNS)} runs), tools/b7_suite.py")
    print("stack 2000000")
    print("FailAt 30")
    print(f"MakeDir {root}/data/Saves {saves}")
    # The autosaves go beside the runs' saves, and which of the two slots a
    # run writes depends on the files already there (`GetOldestSlotFileName`,
    # by file time): start from none, as `host` does in a new directory. A
    # run of only some ids compares with a host run of the same ids into a
    # new directory, never with a full baseline.
    print(f"Delete {saves}/autosave-#?.sav QUIET")
    for run in runs:
        words = args_of(run, lambda s: f"{saves}/{s}.sav")
        script = f"{root}/scripts/{run[1]}.script"
        print(f"{program} >RESULTS:{prefix}-{run[0]}.log window 640x480 {' '.join(words)} script {script} do quit")
    print(f"Echo done >RESULTS:{prefix}-done")


def host(outdir, binary, only=()):
    out = Path(outdir).resolve()
    saves = out / "saves"
    saves.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, AROS_GAME_BIN=str(Path(binary).resolve()))
    for run in RUNS:
        if only and run[0] not in only:
            continue
        words = args_of(run, lambda s: str(saves / f"{s}.sav"))
        d = out / run[0]
        subprocess.run(["/bin/rm", "-rf", str(d)])
        subprocess.run([str(REPO / "scripts/headless.sh"), str(d), "900",
                        str(REPO / "scripts" / f"{run[1]}.script"), "window", "640x480"] + words,
                       env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        log = d / f"{run[1]}.log"
        (out / f"{run[0]}.log").write_bytes(log.read_bytes() if log.exists() else b"NO LOG\n")
        print(run[0], flush=True)


def normal(text):
    keep = []
    for line in text.splitlines():
        line = line.rstrip("\r")
        # Where the pack came from: the data drawer on AROS (the release reads
        # the player's), built in on the host.
        if line.startswith(("wrote ", "renderer:", "game data: ")):
            continue
        # How many game controllers SDL found: the machine's, not the game's
        # (mainline's SDL2 reports one on the QEMU PC and the Pi; the host and
        # ABIv11 none).
        if line.startswith("game controllers: "):
            continue
        # The Modern opponent's timing telemetry (`AiTelemetry`, upstream's
        # thresholds: 10 ms for FindBestTradeCity and Trading.AddCandidates'
        # breakdown, 1 s for a turn): wall-clock time of the machine running
        # it, reported, never acted on. A slower guest (the QEMU Pi) crosses
        # them where the host does not.
        if re.match(r"^modern ai: player \d+ (FindBestTradeCity took \d+ ms|Trading\.AddCandidates took \d+ ms|slow AI turn \d+ ms)", line):
            continue
        # Where a save lies differs by system; its name and digest do not.
        line = re.sub(r"(saved|loaded) day (\d+) (to|from) \S*/([^/\s]+\.sav)",
                      r"\1 day \2 \3 \4", line)
        line = re.sub(r"^autosave: day (\d+) to \S*/([^/\s]+\.sav)", r"autosave: day \1 to \2", line)
        # A song's position follows the audio thread's real time.
        line = re.sub(r"^(music: .*) @[0-9]+$", r"\1 @POS", line)
        keep.append(line)
    return keep


def compare(hostdir, arosdir, prefix):
    same = 0
    for run in RUNS:
        h = Path(hostdir) / f"{run[0]}.log"
        a = Path(arosdir) / f"{prefix}-{run[0]}.log"
        if not a.exists():
            print(f"{run[0]}: no AROS log")
            continue
        hl, al = normal(h.read_text(errors="replace")), normal(a.read_text(errors="replace"))
        if hl == al:
            same += 1
            marks = [m for m in re.findall(r"\(([0-9a-f]{16})\)", "\n".join(al))]
            print(f"{run[0]}: same ({len(al)} lines, saves {' '.join(marks) or 'none'})")
        else:
            first = next(i for i in range(max(len(hl), len(al)))
                         if i >= len(hl) or i >= len(al) or hl[i] != al[i])
            print(f"{run[0]}: DIFFERS at line {first + 1}: host {hl[first] if first < len(hl) else '<end>'!r}"
                  f" / AROS {al[first] if first < len(al) else '<end>'!r}")
    print(f"{same} of {len(RUNS)} the same")


if __name__ == "__main__":
    mode = sys.argv[1] if len(sys.argv) > 1 else ""
    if mode == "aros" and len(sys.argv) >= 4:
        aros(sys.argv[2], sys.argv[3], sys.argv[4:])
    elif mode == "host" and len(sys.argv) >= 3:
        host(sys.argv[2], sys.argv[3] if len(sys.argv) >= 4 else str(REPO / "target/debug/aros-game"), sys.argv[4:])
    elif mode == "compare" and len(sys.argv) == 5:
        compare(sys.argv[2], sys.argv[3], sys.argv[4])
    else:
        sys.exit(__doc__)
