#!/usr/bin/env python3
"""Text selection and the clipboard on Windows, by real pointer presses and
keys in `text_selection`'s editors:

- a double click at the system's double-click time (`GetDoubleClickTime`)
  selects a word, and a second press later than it does not;
- a second press outside the system's double-click box (`SM_CXDOUBLECLK` by
  `SM_CYDOUBLECLK`) is a new first press; inside it, a double click;
- a triple click selects the whole line;
- copied lines end in CR LF, as Windows apps take them (read raw from the
  clipboard), and CR LF text pasted in from another process leaves no CR in
  the editor.

Selections are read through UI Automation (`windows_uia_dump.ps1`). The
user's clipboard is saved first and restored after, and what the smoke puts
there itself stays out of clipboard history; with cloud clipboard sync on,
the copy and paste checks are skipped, so nothing copied leaves the machine.

    cargo build -p zgx-desktop --example text_selection
    python scripts/windows_text_smoke.py target/debug/examples/text_selection.exe
"""
import argparse
import ctypes
import pathlib
import re
import subprocess
import threading
import time
import winreg
from ctypes import wintypes

import windows_input as win

user32 = ctypes.WinDLL("user32", use_last_error=True)
kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
for name, res, args in (
    ("OpenClipboard", wintypes.BOOL, [wintypes.HWND]),
    ("CloseClipboard", wintypes.BOOL, []),
    ("EmptyClipboard", wintypes.BOOL, []),
    ("EnumClipboardFormats", wintypes.UINT, [wintypes.UINT]),
    ("GetClipboardData", wintypes.HANDLE, [wintypes.UINT]),
    ("SetClipboardData", wintypes.HANDLE, [wintypes.UINT, wintypes.HANDLE]),
    ("RegisterClipboardFormatW", wintypes.UINT, [wintypes.LPCWSTR]),
    ("GetDoubleClickTime", wintypes.UINT, []),
    ("GetSystemMetricsForDpi", ctypes.c_int, [ctypes.c_int, wintypes.UINT]),
):
    getattr(user32, name).restype, getattr(user32, name).argtypes = res, args
for name, res, args in (
    ("GlobalAlloc", wintypes.HANDLE, [wintypes.UINT, ctypes.c_size_t]),
    ("GlobalLock", ctypes.c_void_p, [wintypes.HANDLE]),
    ("GlobalUnlock", wintypes.BOOL, [wintypes.HANDLE]),
    ("GlobalSize", ctypes.c_size_t, [wintypes.HANDLE]),
):
    getattr(kernel32, name).restype, getattr(kernel32, name).argtypes = res, args
CF_UNICODETEXT = 13
# Formats whose data is a GDI or metafile handle, not memory to copy.
HANDLE_FORMATS = {2, 3, 9, 14, 0x80, 0x82, 0x83, 0x8E}
# Clipboard history and cloud sync pass over data carrying this format.
NO_HISTORY = user32.RegisterClipboardFormatW("ExcludeClipboardContentFromMonitorProcessing")


def open_clipboard():
    for _ in range(50):
        if user32.OpenClipboard(None):
            return
        time.sleep(0.02)
    raise RuntimeError("the clipboard stayed open in another process")


def save_clipboard():
    """Every memory-backed format on the clipboard: (format, bytes)."""
    open_clipboard()
    try:
        saved, fmt = [], 0
        while fmt := user32.EnumClipboardFormats(fmt):
            if fmt in HANDLE_FORMATS:
                continue
            handle = user32.GetClipboardData(fmt)
            size = kernel32.GlobalSize(handle) if handle else 0
            if size:
                pointer = kernel32.GlobalLock(handle)
                saved.append((fmt, ctypes.string_at(pointer, size)))
                kernel32.GlobalUnlock(handle)
        return saved
    finally:
        user32.CloseClipboard()


def put_clipboard(items):
    """Replace the clipboard with `items` ((format, bytes)), kept out of
    clipboard history."""
    open_clipboard()
    try:
        user32.EmptyClipboard()
        for fmt, data in [*items, (NO_HISTORY, b"\0\0\0\0")]:
            handle = kernel32.GlobalAlloc(0x2, len(data))  # GMEM_MOVEABLE
            ctypes.memmove(kernel32.GlobalLock(handle), data, len(data))
            kernel32.GlobalUnlock(handle)
            user32.SetClipboardData(fmt, handle)
    finally:
        user32.CloseClipboard()


def clipboard_text():
    """CF_UNICODETEXT exactly as it is, CR LF and all."""
    open_clipboard()
    try:
        handle = user32.GetClipboardData(CF_UNICODETEXT)
        if not handle:
            return None
        pointer = kernel32.GlobalLock(handle)
        text = ctypes.wstring_at(pointer)
        kernel32.GlobalUnlock(handle)
        return text
    finally:
        user32.CloseClipboard()


def cloud_clipboard():
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Software\Microsoft\Clipboard") as key:
            return winreg.QueryValueEx(key, "EnableCloudClipboard")[0] == 1
    except OSError:
        return False


def editors(window):
    """name -> (client rect in pixels, text selection) of each editor."""
    dump = subprocess.run(["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                           str(pathlib.Path(__file__).with_name("windows_uia_dump.ps1")),
                           "-Hwnd", str(getattr(window, "value", window)), "-Rects"],
                          capture_output=True, text=True, encoding="utf-8").stdout
    ox, oy = win.to_screen(window, 0, 0)
    found = {}
    for line in dump.splitlines():
        match = re.match(r'\s*Edit name="((?:[^"\\]|\\.)*)".*?(?: text-selection="((?:[^"\\]|\\.)*)")?'
                         r' rect=(-?\d+),(-?\d+),(\d+),(\d+)', line)
        if match:
            name, selection, x, y, w, h = match.groups()
            found[name] = ((int(x) - ox, int(y) - oy, int(w), int(h)),
                           (selection or "").encode().decode("unicode_escape"))
    return found


def selection(window, name, expected=None, timeout=1.5):
    """The editor's selection, once it is `expected` (UI Automation's names
    and values follow at most 100 ms behind while no screen reader runs)."""
    deadline = time.monotonic() + timeout
    while True:
        found = editors(window)[name][1]
        if expected is None or found == expected or time.monotonic() > deadline:
            return found
        time.sleep(0.1)


def press(window, x, y):
    win.click(window, x, y)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("binary", type=pathlib.Path)
    args = parser.parse_args()
    interval = user32.GetDoubleClickTime() / 1000
    box = (user32.GetSystemMetricsForDpi(36, 96), user32.GetSystemMetricsForDpi(37, 96))  # SM_CX/CYDOUBLECLK
    proc = subprocess.Popen([str(args.binary)], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                            encoding="utf-8")
    lines = []
    threading.Thread(target=lambda: lines.extend(proc.stdout), daemon=True).start()
    saved = save_clipboard()
    results = []
    try:
        window = win.wait_window(proc)
        time.sleep(1)
        win.activate(window)
        scale = win.dpi(window) / 96
        fields = editors(window)
        (x, y, w, h), _ = fields["Words"]
        # "these": past "select " (about 50 points at 16 points).
        word_x, word_y = x + (10 + 68) * scale, y + h / 2
        (lx, ly, lw, lh), _ = fields["Lines"]
        elsewhere = (lx + lw / 2, ly + lh - 10 * scale)

        def reset():
            press(window, *elsewhere)
            time.sleep(interval * 1.5)

        # Within the double-click time: a word.
        reset()
        press(window, word_x, word_y)
        time.sleep(interval * 0.4)
        press(window, word_x, word_y)
        got = selection(window, "Words", "these")
        assert got == "these", f"a double click within {interval * 1000:.0f} ms selected {got!r}"
        results.append(f"double click {interval * 0.4 * 1000:.0f} ms apart: {got!r}")
        # Later than it: no word.
        reset()
        press(window, word_x, word_y)
        time.sleep(interval * 1.6)
        press(window, word_x, word_y)
        time.sleep(0.4)
        got = selection(window, "Words")
        assert got == "", f"presses {interval * 1.6 * 1000:.0f} ms apart selected {got!r}"
        results.append(f"presses {interval * 1.6 * 1000:.0f} ms apart: nothing selected")
        # Outside the box, then inside it.
        outside = (box[0] / 2 + 1) * scale
        reset()
        press(window, word_x, word_y)
        press(window, word_x + outside, word_y)
        time.sleep(0.4)
        got = selection(window, "Words")
        assert got == "", f"a second press {outside:.0f} px off, outside the {box} box, selected {got!r}"
        reset()
        inside = max(box[0] / 2 - 1, 0) * scale
        press(window, word_x, word_y)
        press(window, word_x + inside, word_y)
        got = selection(window, "Words", "these")
        assert got == "these", f"a second press {inside:.0f} px off, inside the {box} box, selected {got!r}"
        results.append(f"{box[0]}x{box[1]} box: {outside:.0f} px off a new press, {inside:.0f} px off a double")
        # Three presses: the line.
        reset()
        win.click(window, word_x, word_y, count=3, delay=interval * 0.25)
        got = selection(window, "Words", "select these words")
        assert got == "select these words", f"a triple click selected {got!r}"
        results.append(f"triple click: {got!r}")
        if cloud_clipboard():
            results.append("copy and paste skipped: cloud clipboard sync is on")
        else:
            # Copied lines: CR LF.
            press(window, *elsewhere)
            win.key("ctrl+a", "ctrl+c")
            time.sleep(0.4)
            copied = clipboard_text()
            assert copied == "first line\r\nsecond line", f"copied {copied!r}"
            results.append(f"copied {copied!r}")
            # Pasted CR LF: no CR in the editor.
            put_clipboard([(CF_UNICODETEXT, "pasted\r\nlines\0".encode("utf-16-le"))])
            press(window, *elsewhere)
            win.key("ctrl+a", "ctrl+v")
            time.sleep(0.4)
        win.close(window)
        proc.wait(timeout=10)
        time.sleep(0.2)
        report = next((line for line in lines if line.startswith("TEXT ")), "")
        if not cloud_clipboard():
            assert 'lines="pasted\\nlines"' in report, f"the paste reached the editor as: {report!r}"
            results.append(f"pasted CR LF, the editor holds {report.split('lines=')[1].strip()}")
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
        put_clipboard(saved)
    for line in results:
        print(f"  {line}")
    print(f"PASS: text selection at {interval * 1000:.0f} ms and a {box[0]}x{box[1]} box; clipboard line ends")


if __name__ == "__main__":
    main()
