# Windows desktop support

Use stable Rust with the `x86_64-pc-windows-msvc` target and Visual Studio Build
Tools (Desktop development with C++, including the Windows SDK). From PowerShell:

```powershell
cargo run -p zgui-desktop --example components --locked
cargo run -p zgui-desktop --example platform_services --locked
```

Windows uses the existing winit event loop and wgpu renderer (DX12/Vulkan),
arboard clipboard, AccessKit Windows accessibility backend, and rfd native file
pickers and message boxes. Both file pickers and prompts receive their owning
native window. Application menus use `cx.app_menu_bar()` and Control shortcuts.
No additional Windows-only dependency is needed.

## Presentation

wgpu runs on Direct3D 12 alone; Vulkan stands in only where Direct3D 12 has
no adapter (`WGPU_BACKEND` still overrides). Swap chains use DXGI's flip model
at sync interval 0: a present replaces one the compositor has not taken yet,
without tearing, so input draws at once and shows on the next refresh even
while an animation runs. Animation frames start half a refresh after the
compositor's clock ticks (`DCompositionWaitForCompositorClock` on Windows 11,
`DwmFlush` on Windows 10), far from the point where it takes them, on one
thread that parks while nothing animates; a window at rest wakes for nothing.
Measured on an RTX 4090 at 60 Hz:

| | before | now |
| --- | --- | --- |
| Gallery at rest | 10.6 M cycles/s, Vulkan and D3D12 drivers loaded | 2.7 M cycles/s (the driver's own threads) |
| Keystroke to composed screen, at rest | 12â€“14 ms median | 12â€“14 ms median |
| Keystroke to composed screen, animating | 47 ms median | 12 ms median |
| Animation at the display's rate | 57.7â€“60 fps, timer drifting against refreshes | every refresh, none repeated or dropped |

`ZGUI_PRESENT_LOG=1` logs each shown present as DXGI reports it. The probes
measure the same things on any machine:

```powershell
cargo build --release -p zgui-desktop --example gallery --example input_latency --example animation_workload
python scripts/windows_idle_probe.py target/release/examples/gallery.exe
python scripts/windows_latency_probe.py target/release/examples/input_latency.exe --click 100,44 --region 24,24,520,40 --env ZGUI_ANIMATE=1
python scripts/windows_pacing_probe.py target/release/examples/animation_workload.exe
```

## Title bars

`Titlebar::Inset` and `Titlebar::Hidden` are frameless windows that keep the
system shadow and Windows 11's rounded corners, resizing from bands 5 points in
from their edges. `Inset` adds Windows 11's caption buttons over the content's
top right, drawn by zgui and reported to Windows as its own (`WM_NCHITTEST`),
so they get the system's tooltips and the maximize button's snap layouts; the
buttons minimize, maximize, restore and close through the system's commands.
The title bar follows the system's light or dark mode.

Windows present through a DirectComposition visual, so `WindowOptions::transparent`
windows are transparent where zgui paints nothing, and `WindowOptions::glass`
puts the Windows 11 system backdrop behind them: Acrylic for the heads-up and
transient materials, Mica for window and page backgrounds, Mica Alt for title
bars (see [window controls](window-controls.md)). Presentation through the visual
measured the same as a window's own swap chain: every frame shown, the same
keystroke latency and idle cost.
`scripts/windows_chrome_smoke.py` checks the buttons and the resize bands.

## System settings

The built-in themes take these from Windows, read at startup and again when
the user changes them (`WM_SETTINGCHANGE`, `WM_SYSCOLORCHANGE`), in every
window whatever its title bar:

- The accent colour (Settings, Personalization, Colors): text selections are
  drawn in it with white text, as WinUI's text fields and Notepad draw them.
- High contrast (contrast themes): backgrounds, surfaces, text, borders, the
  accent, selections and hover take the contrast theme's system colours.
- Animation effects off (Accessibility, Visual effects): the motion duration
  tokens are zero, so what animates with them changes at once.
- Text size (Accessibility): the font size and line height tokens scale with
  it, 100% to 225%, and spacing and icons keep their size. A subtree whose
  layout depends on fixed text, like a terminal grid, opts out with
  `ThemeOverride::fixed_text_size`.

An app theme or `theme_scope` that sets a colour or duration token itself
keeps its own value; the text size scales the font tokens of every theme.

## Limitations

- `Application::application_id` opts into OS activation callbacks, which are not
  implemented on Windows. Running an application with an ID returns an explicit
  error. Handle launch arguments and protocol registration in the application.
- Cancelling a file-picker or prompt future invalidates result delivery but
  does not dismiss its native dialog. A pending native dialog can retain its
  parent until dismissed. Backend failures can appear as cancellation.
- Rendered application menus are supported; native Win32 menu bars are not.
- Native views (`web_view`, Ghostty terminals) are macOS only.
- The Linux benchmark sampler and the X11/Wayland smoke scripts do not run on
  Windows; Windows has its own (below).

## Smoke checks

`scripts/windows_*_smoke.py` drive an example through real Win32 input
(`SendInput`, so winit sees the messages a keyboard and mouse send) and check
what it reports. They need only Python's standard library and an unlocked,
interactive desktop, and take the pointer and keyboard while they run:

```powershell
cargo build -p zgui-desktop --example gallery --example dpi_transition --example native_ime --example selectable_text --example platform_services --example drag_drop
python scripts/windows_desktop_smoke.py target/debug/examples/gallery.exe
python scripts/windows_dpi_smoke.py target/debug/examples/dpi_transition.exe
python scripts/windows_ime_smoke.py target/debug/examples/native_ime.exe --install-korean
python scripts/windows_platform_smoke.py target/debug/examples
```

- `windows_desktop_smoke.py`: pointer activation, typing, selection, clipboard,
  Tab focus, resize and close in the gallery.
- `windows_dpi_smoke.py`: the display's scale goes 100% to 150% and back while
  `dpi_transition` runs (as Settings changes it, for the signed-in user, and
  restored after). Each scale must reach the window as per-monitor DPI, scale
  its physical size, keep its layout in logical pixels and take pointer and
  typing at the scaled positions.
- `windows_ime_smoke.py`: the Microsoft Korean IME composes hangul from real
  keystrokes (preedit, commit), and a click into another editor mid-composition
  commits it where it began. Korean composes without a dictionary, so it works
  on any install; `--install-korean` adds it to the user's input methods for the
  run and restores the list after. The Japanese and Chinese IMEs need their
  language's typing features (an administrator install) before they compose.
  `native_ime --open-ime` opens the IME itself: input methods ignore a
  synthesized on/off key.
- `windows_platform_smoke.py`: selected display text copied as text and HTML
  and read by another process, text from another process pasted into an
  editor, the file picker and the message box (each owned by its window, the
  window disabled while it shows), and a file dragged in from another process
  through OLE drag and drop, as from Explorer.

`scripts/windows_uia_dump.ps1 -Hwnd <window>` prints a window's UI Automation
tree as Narrator reads it: control types, names, values, toggle and range
states, and text selections.
