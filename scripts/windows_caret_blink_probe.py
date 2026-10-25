#!/usr/bin/env python3
"""What a blinking caret costs on Windows: `text_selection`'s editor
clicked into, then its process's CPU cycles (`QueryProcessCycleTime`) and
frames (`ZGX_PRESENT_LOG`) counted while the caret blinks, and again once
it has stopped at the system's caret timeout with the field still focused.

The caret blinks at `GetCaretBlinkTime` each way and stops after the caret
timeout (`SPI_GETCARETTIMEOUT`); each blink must be one small frame, and the
focused field at rest after it must do no work. With the caret timeout off
(`INFINITE`) it blinks for as long as the field has the focus; the idle
measure is then skipped.

    cargo build --release -p zgx-desktop --example text_selection
    python scripts/windows_caret_blink_probe.py target/release/examples/text_selection.exe
"""
import argparse
import ctypes
import os
import pathlib
import statistics
import subprocess
import tempfile
import time

import windows_input as win
import windows_text_smoke as text

user32 = ctypes.WinDLL("user32")


def caret_timeout():
    timeout = ctypes.c_uint(0)
    if not user32.SystemParametersInfoW(0x2022, 0, ctypes.byref(timeout), 0):  # SPI_GETCARETTIMEOUT
        return None
    return None if timeout.value in (0, 0xFFFFFFFF) else timeout.value / 1000


def renders(log):
    return sum(1 for line in pathlib.Path(log).read_text(errors="replace").splitlines() if " render " in line)


def measure(binary, blinking, resting):
    """(cycles a blink, frames while blinking, idle cycles a second and
    frames at rest), from one fresh process."""
    with tempfile.TemporaryDirectory(prefix="zgx-caret-") as directory:
        log = os.path.join(directory, "present.log")
        proc = subprocess.Popen([str(binary)], stdout=subprocess.DEVNULL, stderr=open(log, "w"),
                                env=dict(os.environ, ZGX_PRESENT_LOG="1"))
        try:
            window = win.wait_window(proc)
            time.sleep(1.5)
            win.activate(window)
            (x, y, w, h), _ = text.editors(window)["Words"]
            # The same window at rest with nothing focused, to compare.
            time.sleep(1)
            cycles = win.cycles(proc.pid)
            time.sleep(5)
            unfocused = (win.cycles(proc.pid) - cycles) / 5
            win.click(window, x + 10 + 45, y + h / 2)
            clicked = time.perf_counter()
            # The click's own frames done: the caret shown, solid.
            time.sleep(0.3)
            frames, cycles = renders(log), win.cycles(proc.pid)
            time.sleep(blinking)
            win.require_foreground(window)
            blinks = renders(log) - frames
            blinking_cycles = win.cycles(proc.pid) - cycles
            rest = None
            if resting:
                # Past the caret timeout, still focused.
                time.sleep(max(resting[0] - (time.perf_counter() - clicked), 0) + 1)
                frames, cycles = renders(log), win.cycles(proc.pid)
                time.sleep(resting[1])
                win.require_foreground(window)
                rest = ((win.cycles(proc.pid) - cycles) / resting[1], renders(log) - frames)
            win.close(window)
            proc.wait(timeout=10)
        finally:
            if proc.poll() is None:
                proc.kill()
                proc.wait()
    idle = rest[0] if rest else unfocused
    per_blink = (blinking_cycles - idle * blinking) / blinks if blinks else None
    return per_blink, blinks, rest, unfocused


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--runs", type=int, default=3)
    args = parser.parse_args()
    phase = user32.GetCaretBlinkTime()
    if phase in (0, 0xFFFFFFFF):
        raise SystemExit("the caret does not blink on this system (blink rate: none)")
    timeout = caret_timeout()
    # Blinking: within the timeout. Resting: 10 s past it.
    blinking = min(4.0, (timeout or 6) - 0.8)
    resting = (timeout, 10.0) if timeout else None
    rows = [measure(args.binary, blinking, resting) for _ in range(args.runs)]
    expected = blinking / (phase / 1000)
    for per_blink, blinks, rest, _ in rows:
        assert blinks >= expected * 0.7, f"{blinks} frames in {blinking:.1f} s at {phase} ms a phase"
        if rest:
            assert rest[1] == 0, f"{rest[1]} frames with the caret at rest past its timeout"
    per_blink = statistics.median(r[0] for r in rows)
    print(f"blink: {phase} ms each way, caret timeout {f'{timeout:.0f} s' if timeout else 'none'}")
    print(f"  {statistics.median(r[1] for r in rows):.0f} frames in {blinking:.1f} s blinking; "
          f"{per_blink / 1e6:.2f} M cycles a blink (median of {args.runs})")
    if resting:
        idle = statistics.median(r[2][0] for r in rows)
        unfocused = statistics.median(r[3] for r in rows)
        print(f"  at rest past the timeout, focused: {idle / 1e6:.3f} M cycles a second, "
              f"{max(r[2][1] for r in rows)} frames in {resting[1]:.0f} s "
              f"(nothing focused: {unfocused / 1e6:.3f} M cycles a second)")
    print("PASS: the caret blinks one small frame a phase and rests at its timeout")


if __name__ == "__main__":
    main()
