#!/usr/bin/env python3
"""Startup to first frame on Windows, the same way for every framework: spawn
the binary, wait for its window, then read its header strip until the
header's text has drawn (light pixels on the dark background). Reports, per
binary, the time from spawn to the window and to the text, and the process's
CPU time and cycles at that point, over `--launches` launches in rotating
order. With `--settle`, each process then runs on, idle, that many seconds,
and its private bytes and working set are taken: the memory it holds at rest.

    python scripts/windows_startup_probe.py target/dist/examples/component_workload.exe comparisons/gpui-fast/target/dist/zgx-compare-gpui-fast.exe comparisons/mygo/bin/lab.exe --settle 5

The comparison adapters (zgx, GPUI Fast, MyGo) all draw "zgx performance
lab" at (20, 20) logical pixels, 20 px. The window is read as it drew
itself (`PrintWindow`), so another window covering it cannot pass for its
text. Medians of cycles say more than wall times, which swing with the
system and the compositor.
"""
import argparse
import os
import pathlib
import statistics
import subprocess
import time

import windows_input as win


def drawn(window, scale):
    x, y, w, h = (round(v * scale) for v in (20, 20, 240, 28))
    width, height, data = win.capture_window(window)
    light = 0
    for row in range(y, min(y + h, height)):
        for column in range(x, min(x + w, width), 2):
            b, g, r, _ = data[(row * width + column) * 4:(row * width + column) * 4 + 4]
            if min(r, g, b) > 170:
                light += 1
    return light > 30


def launch(binary, settle):
    # ZGUI_*: adapters built before the rename read those alone.
    env = dict(os.environ, ZGX_MODE="idle", ZGX_SECONDS=str(settle + 8), ZGX_INITIAL_TICKS="0",
               ZGUI_MODE="idle", ZGUI_SECONDS=str(settle + 8), ZGUI_INITIAL_TICKS="0")
    started = time.perf_counter()
    proc = subprocess.Popen([binary], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        window = None
        while window is None:
            if time.perf_counter() - started > 20:
                raise RuntimeError(f"{binary}: no window")
            found = win.windows_of(proc.pid)
            window = found[0] if found else None
        shown = time.perf_counter() - started
        scale = win.dpi(window) / 96
        while not drawn(window, scale):
            if time.perf_counter() - started > 20:
                raise RuntimeError(f"{binary}: the header never drew")
        first = time.perf_counter() - started
        startup = (shown * 1000, first * 1000, win.cpu_seconds(proc.pid) * 1000, win.cycles(proc.pid) / 1e6)
        if not settle:
            return startup
        time.sleep(settle)
        private, working = win.memory(proc.pid)
        return (*startup, private / 2**20, working / 2**20)
    finally:
        proc.kill()
        proc.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("binaries", nargs="+")
    parser.add_argument("--launches", type=int, default=10)
    parser.add_argument("--settle", type=float, default=0,
                        help="seconds at rest after the first frame before taking its memory")
    args = parser.parse_args()
    binaries = [str(pathlib.Path(b).resolve()) for b in args.binaries]
    results = {b: [] for b in binaries}
    for launch_index in range(args.launches):
        # A different one first each round: none always starts after the same.
        shift = launch_index % len(binaries)
        for binary in binaries[shift:] + binaries[:shift]:
            results[binary].append(launch(binary, args.settle))
            time.sleep(0.5)
    for binary, runs in results.items():
        column = lambda i: statistics.median(run[i] for run in runs)
        memory = (f", at rest {column(4):.1f} MiB private, {column(5):.1f} MiB working set"
                  if args.settle else "")
        print(f"{pathlib.Path(binary).name}: window {column(0):.0f} ms, first text {column(1):.0f} ms, "
              f"CPU {column(2):.0f} ms, {column(3):.0f} M cycles by then{memory} (medians of {len(runs)})")


if __name__ == "__main__":
    main()
