#!/usr/bin/env python3
"""The vlist scene on Windows, as the macOS runner checks it
(`comparisons/README.md`, `scripts/blankprobe.swift`):

- before anything else, each framework's list measures the probe rows
  (`ZGX_MODE=probe`): their heights must agree within 2 points, in the same
  font file, or the frameworks are not drawing the same thing;
- then, in each mode (`scroll`, `fling`, `jump`), the window is captured 10
  times a second for 8 seconds after a warm-up, and each capture scanned
  down columns inside every row's left padding, where a drawn row always
  shows its own background (`vlist::ROW_COLORS`): a run closer to the
  window's background (`vlist::BACKGROUND`) than to either row colour,
  longer than 24 points, is a place no row was drawn. zgx must show none;
  the other frameworks' counts are reported.

Every adapter is given the one font file (`--font`, staged as
`ZGX_FONT_FILE`, as the macOS runner stages it) and must report loading it.

The windows are put above the others while they run (topmost, never
activated), so the screen shows them; nothing is typed or clicked.

    python scripts/windows_vlist_smoke.py zgx=target/dist/examples/vlist_workload.exe gpui-fast=comparisons/gpui-fast/target/dist/vlist.exe mygo=comparisons/mygo/bin/vlist.exe
"""
import argparse
import json
import os
import pathlib
import re
import statistics
import subprocess
import time
from ctypes import wintypes

import windows_input as win

BACKGROUND = (0x0E, 0x12, 0x18)
ROWS = ((0x16, 0x1C, 0x26), (0x19, 0x20, 0x2B))
COLUMNS = (4, 8, 12)  # points from the window's left, inside the rows' padding (16)
BLANK = 24.0  # points: rows are at least 50 tall, so a gap between drawn rows is never this long
TOLERANCE = 2.0
WARMUP, SECONDS, RATE = 3.0, 8.0, 10
FONT = pathlib.Path(__file__).resolve().parent.parent / "comparisons" / "fonts" / "DejaVuSans.ttf"
font_file = None


def fnv64(data):
    value = 0xCBF29CE484222325
    for byte in data:
        value = ((value ^ byte) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{value:016x}"


def env(mode, seconds):
    extra = {"ZGX_FONT_FILE": str(font_file), "ZGUI_FONT_FILE": str(font_file)} if font_file else {}
    return dict(os.environ, ZGX_MODE=mode, ZGX_SECONDS=f"{seconds:g}", ZGX_INITIAL_TICKS="0",
                ZGUI_MODE=mode, ZGUI_SECONDS=f"{seconds:g}", ZGUI_INITIAL_TICKS="0", **extra)


def probe(binary):
    """The heights the list measured for the probe rows, its widths and font."""
    run = subprocess.run([binary], env=env("probe", 30), capture_output=True, text=True, timeout=60)
    text = run.stdout + run.stderr
    heights = re.search(r"row_heights=(\S+)", text)
    font = re.search(r"font_file_fnv64=([0-9a-f]{16})", text)
    return {
        "heights": {int(r): float(h) for r, h in (p.split(":") for p in heights.group(1).split(","))} if heights else {},
        "widths": {k: float(v) for k, v in re.findall(r"(list_width|text_width)=([0-9.]+)", text)},
        "font": font.group(1) if font else None,
    }


def distance(a, b):
    return sum((x - y) ** 2 for x, y in zip(a, b))


def longest_blank(shot, scale):
    width, height, data = shot
    longest = 0.0
    for column in COLUMNS:
        x = int(column * scale)
        if x >= width:
            continue
        run = 0
        for y in range(height):
            i = (y * width + x) * 4
            rgb = (data[i + 2], data[i + 1], data[i])
            near = distance(rgb, BACKGROUND)
            if near < distance(rgb, ROWS[0]) and near < distance(rgb, ROWS[1]):
                run += 1
                longest = max(longest, run / scale)
            else:
                run = 0
    return longest


def check_detector():
    """A capture of rows with a stretch of background in them is caught, and
    one of rows alone is not: the scan can see what it looks for."""
    width, height = 40, 200
    data = bytearray()
    for y in range(height):
        color = BACKGROUND if 80 <= y < 120 else ROWS[(y // 50) % 2]
        data += bytes((color[2], color[1], color[0], 255)) * width
    assert longest_blank((width, height, bytes(data)), 1.0) == 40.0
    rows = bytearray()
    for y in range(height):
        color = ROWS[(y // 50) % 2]
        rows += bytes((color[2], color[1], color[0], 255)) * width
    assert longest_blank((width, height, bytes(rows)), 1.0) == 0.0


def blanks(binary, mode):
    proc = subprocess.Popen([binary], env=env(mode, WARMUP + SECONDS + 1), stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL)
    try:
        window = win.wait_window(proc)
        # Above the other windows, without taking the focus.
        win.user32.SetWindowPos(window, wintypes.HWND(-1), 0, 0, 0, 0, 0x1 | 0x2 | 0x10)
        scale = win.dpi(window) / 96
        time.sleep(WARMUP)
        captures = blank = 0
        longest = 0.0
        end = time.perf_counter() + SECONDS
        while time.perf_counter() < end:
            started = time.perf_counter()
            run = longest_blank(win.capture(window), scale)
            captures += 1
            longest = max(longest, run)
            blank += run > BLANK
            time.sleep(max(0.0, 1 / RATE - (time.perf_counter() - started)))
    finally:
        proc.kill()
        proc.wait()
    return {"captures": captures, "blank_captures": blank, "longest_blank_points": round(longest, 1)}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("binaries", nargs="+", help="name=path of each framework's vlist binary; zgx's named zgx")
    parser.add_argument("--font", type=pathlib.Path, default=FONT, help="the one font file every adapter loads")
    parser.add_argument("--output", type=pathlib.Path)
    args = parser.parse_args()
    check_detector()
    global font_file
    font_file = args.font.resolve()
    expected = fnv64(font_file.read_bytes())
    binaries = dict(entry.split("=", 1) for entry in args.binaries)
    binaries = {name: str(pathlib.Path(path).resolve()) for name, path in binaries.items()}
    found = {name: probe(path) for name, path in binaries.items()}
    rows = sorted(set().union(*(f["heights"] for f in found.values())))
    print("probe row " + "".join(f"{name:>11}" for name in found))
    disagree = []
    for row in rows:
        heights = [found[name]["heights"].get(row) for name in found]
        off = None in heights or max(heights) - min(heights) > TOLERANCE
        disagree += [row] if off else []
        print(f"    {row:5d} " + "".join(f"{h:11.1f}" if h is not None else f"{'-':>11}" for h in heights)
              + ("  DISAGREE" if off else ""))
    fonts = {found[name]["font"] for name in found}
    assert rows, f"no framework reported the probe rows: {found}"
    assert not disagree, f"the frameworks measure rows {disagree} differently, beyond {TOLERANCE} points"
    assert fonts == {expected}, f"the adapters loaded {fonts}, not {font_file.name} ({expected})"
    results = {}
    for mode in ("scroll", "fling", "jump"):
        for name, path in binaries.items():
            result = blanks(path, mode)
            results[f"{name}/{mode}"] = result
            print(f"{mode:6} {name:10} {result['blank_captures']:3} of {result['captures']} captures blank, "
                  f"longest {result['longest_blank_points']} points", flush=True)
    if args.output:
        args.output.write_text(json.dumps({"probe": found, "blank": results}, indent=1))
    zgx = {key: r for key, r in results.items() if key.startswith("zgx/")}
    for key, result in zgx.items():
        assert result["captures"] >= SECONDS * RATE * 0.8, f"{key}: only {result['captures']} captures"
        assert result["blank_captures"] == 0, f"{key}: {result['blank_captures']} captures with unfilled rows"
    print(f"PASS: {len(rows)} probe rows agree within {TOLERANCE} points in one font; "
          f"zgx shows no unfilled rows in scroll, fling or jump")


if __name__ == "__main__":
    main()
