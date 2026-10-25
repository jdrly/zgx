#!/usr/bin/env python3
"""Windows' settings reaching a zgx window, driven by SendInput: the window
menu (Alt+Space) is dark in dark mode and light in light mode, and with
"Always show scrollbars" off a scrollbar hides at rest, shows while the user
scrolls and hides again after; with it on it shows at rest.

    cargo build -p zgx-desktop --example scrollbars --example primary_selection
    python scripts/windows_settings_smoke.py target/debug/examples/scrollbars.exe

Checks the user's current settings. `--both` also switches the app mode both
ways while a window runs (its title bar must follow at once), runs with each
setting the other way (the app mode, as Settings switches it, and "Always
show scrollbars"), and restores the user's own after.
"""
import argparse
import ctypes
import pathlib
import subprocess
import time
import winreg
from ctypes import wintypes

import windows_input as win

user32 = ctypes.WinDLL("user32")
user32.SendMessageTimeoutW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPCWSTR,
                                       wintypes.UINT, wintypes.UINT, ctypes.POINTER(wintypes.DWORD)]
EnumProc = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
PERSONALIZE = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"
ACCESSIBILITY = r"Control Panel\Accessibility"
# The vertical scroll view of the scrollbars example, in logical pixels: the
# strip along its right edge where its scrollbar draws.
STRIP = (204, 76, 222, 220)
WHEEL_AT = (124, 148)


def read(path, name):
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, path) as key:
            return winreg.QueryValueEx(key, name)[0]
    except FileNotFoundError:
        return None


def write(path, name, value, broadcast):
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, path, 0, winreg.KEY_SET_VALUE) as key:
        if value is None:
            try:
                winreg.DeleteValue(key, name)
            except FileNotFoundError:
                pass
        else:
            winreg.SetValueEx(key, name, 0, winreg.REG_DWORD, value)
    result = wintypes.DWORD()
    # HWND_BROADCAST, WM_SETTINGCHANGE, as Settings sends it.
    user32.SendMessageTimeoutW(0xFFFF, 0x001A, 0, broadcast, 2, 3000, ctypes.byref(result))


def dark_mode():
    return read(PERSONALIZE, "AppsUseLightTheme") == 0


def always_show_scrollbars():
    # Absent: Windows' default, hidden at rest.
    return read(ACCESSIBILITY, "DynamicScrollbars") == 0


def menu_window():
    found = []

    def each(hwnd, _):
        name = ctypes.create_unicode_buffer(16)
        user32.GetClassNameW(hwnd, name, 16)
        if name.value == "#32768" and user32.IsWindowVisible(hwnd):
            found.append(hwnd)
        return True

    user32.EnumWindows(EnumProc(each), 0)
    return found[0] if found else None


def menu_luminance(window):
    """Mean luminance of the window menu's fill, inside its border."""
    win.require_foreground(window)
    win.key("alt+space")
    deadline = time.monotonic() + 3
    while (menu := menu_window()) is None:
        if time.monotonic() > deadline:
            raise RuntimeError("Alt+Space opened no window menu")
        time.sleep(0.05)
    time.sleep(0.4)
    width, height, data = win.capture(menu)
    samples = [win.pixel((width, height, data), x, y)
               for x in range(width // 4, width * 3 // 4, 3) for y in range(4, height - 4, 3)]
    # The first Escape closes the menu, the second leaves the menu mode the
    # window is left in (as any Win32 window is), which takes the input.
    win.key("escape", "escape")
    time.sleep(0.3)
    return sum(0.2126 * r + 0.7152 * g + 0.0722 * b for r, g, b in samples) / len(samples)


def caption_luminance(window):
    """Mean luminance of the title bar's middle, away from its title and
    buttons, as the screen shows it."""
    rect = wintypes.RECT()
    user32.GetWindowRect(window, ctypes.byref(rect))
    scale = win.dpi(window) / 96
    width = rect.right - rect.left
    shot = win.capture_rect(rect.left + width * 2 // 5, rect.top + round(10 * scale), width // 5,
                            round(14 * scale))
    w, h, _ = shot
    samples = [win.pixel(shot, x, y) for x in range(0, w, 3) for y in range(0, h, 3)]
    return sum(0.2126 * r + 0.7152 * g + 0.0722 * b for r, g, b in samples) / len(samples)


def client_luminance(window):
    """Luminance of the window's own background, at its top left."""
    r, g, b = win.pixel(win.capture(window), 6, 6)
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def live(binary):
    """The app mode switched while the window runs: its title bar
    (DWMWA_USE_IMMERSIVE_DARK_MODE) and its own content follow, at once and
    both ways."""
    if read(r"Software\Microsoft\Windows\DWM", "ColorPrevalence") == 1:
        print("live: title bars show the accent colour in either mode here; title bar not checked")
        return
    mode = read(PERSONALIZE, "AppsUseLightTheme")
    # An example whose theme follows the system's, built beside `binary`;
    # without it, the title bar alone is checked.
    follower = pathlib.Path(binary).with_name("primary_selection.exe")
    content_follows = follower.exists()
    proc = subprocess.Popen([str(follower if content_follows else binary)])
    seen = []
    try:
        window = win.wait_window(proc)
        time.sleep(1.5)
        win.activate(window)
        for _ in range(2):
            dark = not dark_mode()
            write(PERSONALIZE, "AppsUseLightTheme", 0 if dark else 1, "ImmersiveColorSet")
            deadline = time.monotonic() + 4
            while True:
                luminance = caption_luminance(window)
                content = client_luminance(window) if content_follows else (40 if dark else 200)
                if (luminance < 80 and content < 80) if dark else (luminance > 170 and content > 170):
                    break
                if time.monotonic() > deadline:
                    raise AssertionError(f"the title bar (luminance {luminance:.0f}) and content "
                                         f"({content:.0f}) did not turn {'dark' if dark else 'light'} "
                                         "while the window ran")
                time.sleep(0.1)
            seen.append(f"{'dark' if dark else 'light'} (title bar {luminance:.0f}, content {content:.0f})")
        win.close(window)
        proc.wait(timeout=10)
    finally:
        write(PERSONALIZE, "AppsUseLightTheme", mode, "ImmersiveColorSet")
        if proc.poll() is None:
            proc.kill()
            proc.wait()
    print(f"PASS: live mode switches, the title bar and content followed each: {', '.join(seen)}")


def thumb_pixels(window):
    """Pixels of the accent-coloured thumb in the scroll view's right strip."""
    scale = win.dpi(window) / 96
    shot = win.capture(window)
    left, top, right, bottom = (round(v * scale) for v in STRIP)
    return sum(1 for x in range(left, right) for y in range(top, bottom)
               if (lambda r, g, b: b > 180 and b > r + 60)(*win.pixel(shot, x, y)))


def check(binary, dark, always):
    proc = subprocess.Popen([binary])
    try:
        window = win.wait_window(proc)
        time.sleep(1.5)
        win.activate(window)
        win.move(window, 400, 300)
        luminance = menu_luminance(window)
        if dark and luminance > 80 or not dark and luminance < 170:
            raise AssertionError(f"the window menu's fill (luminance {luminance:.0f}) is not "
                                 f"{'dark' if dark else 'light'}")
        rest = thumb_pixels(window)
        win.require_foreground(window)
        win.wheel(window, *WHEEL_AT, -1)
        time.sleep(0.15)
        win.move(window, 400, 300)
        scrolling = thumb_pixels(window)
        time.sleep(2.5)
        after = thumb_pixels(window)
        expected = (50, 50, 50) if always else (0, 50, 0)
        for name, seen, want in zip(("at rest", "while scrolling", "after"), (rest, scrolling, after), expected):
            if (seen >= want) if want else (seen == 0):
                continue
            raise AssertionError(f"scrollbar {name}: {seen} thumb pixels with always-show "
                                 f"{'on' if always else 'off'}")
        win.close(window)
        proc.wait(timeout=10)
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()
    print(f"PASS: {'dark' if dark else 'light'} mode, window menu luminance {luminance:.0f}; always show "
          f"scrollbars {'on' if always else 'off'}, thumb pixels {rest}/{scrolling}/{after} "
          "(rest/scrolling/after)")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary")
    parser.add_argument("--both", action="store_true")
    args = parser.parse_args()
    check(args.binary, dark_mode(), always_show_scrollbars())
    if not args.both:
        return
    live(args.binary)
    mode = read(PERSONALIZE, "AppsUseLightTheme")
    scrollbars = read(ACCESSIBILITY, "DynamicScrollbars")
    try:
        write(PERSONALIZE, "AppsUseLightTheme", 1 if dark_mode() else 0, "ImmersiveColorSet")
        write(ACCESSIBILITY, "DynamicScrollbars", 1 if always_show_scrollbars() else 0, None)
        time.sleep(1)
        check(args.binary, dark_mode(), always_show_scrollbars())
    finally:
        write(PERSONALIZE, "AppsUseLightTheme", mode, "ImmersiveColorSet")
        write(ACCESSIBILITY, "DynamicScrollbars", scrollbars, None)


if __name__ == "__main__":
    main()
