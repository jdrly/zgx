#!/usr/bin/env python3
"""Windows' contrast themes reaching a running zgx window, switched live as
the Left Alt+Left Shift+Print Screen shortcut switches them
(`SPI_SETHIGHCONTRAST`): with a contrast theme on, `text_selection`'s
background is the theme's window colour (`COLOR_WINDOW`), its text the
theme's text colour (`COLOR_WINDOWTEXT`) and a double-clicked word's
selection the theme's highlight (`COLOR_HIGHLIGHT`); turned off, the window
goes back to the colours it had.

It switches the signed-in user's contrast theme for the run, about ten
seconds, which repaints every window on the desktop, and restores the
user's own settings after, the shortcut's flags included. With a contrast
theme already on, it checks that one and switches nothing.

    cargo build -p zgx-desktop --example text_selection
    python scripts/windows_contrast_smoke.py target/debug/examples/text_selection.exe
"""
import argparse
import ctypes
import pathlib
import subprocess
import time
from ctypes import wintypes

import windows_input as win
import windows_text_smoke as text

user32 = ctypes.WinDLL("user32", use_last_error=True)
user32.GetSysColor.restype = wintypes.DWORD
COLOR_WINDOW, COLOR_WINDOWTEXT, COLOR_HIGHLIGHT = 5, 8, 13
HCF_HIGHCONTRASTON = 1


class HIGHCONTRASTW(ctypes.Structure):
    _fields_ = [("cbSize", wintypes.UINT), ("dwFlags", wintypes.DWORD), ("lpszDefaultScheme", wintypes.LPWSTR)]


def contrast():
    settings = HIGHCONTRASTW(cbSize=ctypes.sizeof(HIGHCONTRASTW))
    if not user32.SystemParametersInfoW(0x42, settings.cbSize, ctypes.byref(settings), 0):  # SPI_GETHIGHCONTRAST
        raise ctypes.WinError(ctypes.get_last_error())
    return settings.dwFlags, settings.lpszDefaultScheme


def set_contrast(flags, scheme):
    settings = HIGHCONTRASTW(cbSize=ctypes.sizeof(HIGHCONTRASTW), dwFlags=flags, lpszDefaultScheme=scheme)
    # SPI_SETHIGHCONTRAST, SPIF_UPDATEINIFILE | SPIF_SENDCHANGE
    if not user32.SystemParametersInfoW(0x43, settings.cbSize, ctypes.byref(settings), 3):
        raise ctypes.WinError(ctypes.get_last_error())


def system_color(index):
    bgr = user32.GetSysColor(index)
    return bgr & 0xFF, (bgr >> 8) & 0xFF, (bgr >> 16) & 0xFF


def near(a, b, tolerance=24):
    return all(abs(x - y) <= tolerance for x, y in zip(a, b))


def share(capture, rect, color, tolerance=24):
    """The share of `rect`'s pixels within `tolerance` of `color`."""
    x, y, w, h = (int(v) for v in rect)
    hits = total = 0
    for py in range(max(y, 0), y + h, 2):
        for px in range(max(x, 0), x + w, 2):
            total += 1
            hits += near(win.pixel(capture, px, py), color, tolerance)
    return hits / max(total, 1)


def wait_for(check, what, timeout=10.0):
    deadline = time.monotonic() + timeout
    while not check():
        if time.monotonic() > deadline:
            raise AssertionError(f"timed out waiting for {what}")
        time.sleep(0.2)


def check_contrast(window, results):
    window_color, text_color, highlight = (system_color(i) for i in (COLOR_WINDOW, COLOR_WINDOWTEXT,
                                                                     COLOR_HIGHLIGHT))
    corner = (4, 4)
    wait_for(lambda: near(win.pixel(win.capture(window), *corner), window_color, 8),
             f"the background to take the contrast theme's window colour {window_color}")
    fields = text.editors(window)
    (x, y, w, h), _ = fields["Words"]
    scale = win.dpi(window) / 96
    word = (x + (10 + 68) * scale, y + h / 2)
    win.click(window, *word, count=2, delay=0.05)
    text.selection(window, "Words", "these")
    time.sleep(0.3)
    shot = win.capture(window)
    # The selected word: a block of the highlight colour inside the editor.
    selected = share(shot, (x, y, w, h), highlight, 8)
    assert selected > 0.03, f"{selected:.1%} of the editor is the highlight colour {highlight}"
    # The unselected lines' glyphs: the text colour, against the window's.
    (lx, ly, lw, lh), _ = fields["Lines"]
    glyphs = share(shot, (lx, ly, lw, lh), text_color, 48)
    assert glyphs > 0.005, f"{glyphs:.2%} of the lines editor is the text colour {text_color}"
    results.append(f"contrast on: background {window_color}, text {glyphs:.1%} of the lines editor in "
                   f"{text_color}, a selected word {selected:.1%} of its editor in {highlight}")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--scheme", default="High Contrast Black", help="the contrast theme to switch on")
    args = parser.parse_args()
    flags, scheme = contrast()
    already = bool(flags & HCF_HIGHCONTRASTON)
    proc = subprocess.Popen([str(args.binary)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    results = []
    switched = False
    try:
        window = win.wait_window(proc)
        time.sleep(1)
        win.activate(window)
        before = win.pixel(win.capture(window), 4, 4)
        if not already:
            set_contrast(flags | HCF_HIGHCONTRASTON, args.scheme)
            switched = True
            win.activate(window)
        check_contrast(window, results)
        if switched:
            set_contrast(flags, scheme)
            switched = False
            wait_for(lambda: near(win.pixel(win.capture(window), 4, 4), before, 8),
                     f"the background to go back to {before}")
            results.append(f"contrast off: background back to {before}")
        win.close(window)
        proc.wait(timeout=10)
    finally:
        if switched:
            set_contrast(flags, scheme)
        if proc.poll() is None:
            proc.kill()
            proc.wait()
        if contrast() != (flags, scheme):
            set_contrast(flags, scheme)
    assert contrast() == (flags, scheme), f"the contrast settings are {contrast()}, not the user's {(flags, scheme)}"
    for line in results:
        print(f"  {line}")
    print("PASS: " + ("the user's contrast theme" if already else f"{args.scheme!r} switched on and off live"))


if __name__ == "__main__":
    main()
