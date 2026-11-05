#!/usr/bin/env python3
"""Click in the guest with position and button in ONE input-send-event batch
(press and release each carry the absolute position). vmctl's own click sends
the button alone, which on this QEMU/tablet/AROS setup can land elsewhere.
Usage: bclick.py [r] x y"""
import os, sys, time
# vmctl.py and its pool guard come from the testbench named by `AROS_TOOLS`.
if "AROS_TOOLS" not in os.environ:
    sys.exit("bclick.py: set AROS_TOOLS to the testbench's tools directory")
sys.path.insert(0, os.environ["AROS_TOOLS"])
import vmctl
args = sys.argv[1:]
which = "left"
if args and args[0] == "r":
    which, args = "right", args[1:]
x, y = int(args[0]), int(args[1])
with vmctl.VM() as vm:
    ax, bx, ay, by = vm.calibration()
    gx, gy = int(round((x - bx) / ax)), int(round((y - by) / ay))
    pos = [{"type": "abs", "data": {"axis": "x", "value": gx * vmctl.ABS_MAX // vm.w}},
           {"type": "abs", "data": {"axis": "y", "value": gy * vmctl.ABS_MAX // vm.h}}]
    vm._cmd("input-send-event", events=pos)
    time.sleep(0.15)
    vm._cmd("input-send-event", events=pos + [{"type": "btn", "data": {"down": True, "button": which}}])
    time.sleep(0.12)
    vm._cmd("input-send-event", events=pos + [{"type": "btn", "data": {"down": False, "button": which}}])
    time.sleep(0.3)
