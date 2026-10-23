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
| Keystroke to composed screen, at rest | 12–14 ms median | 12–14 ms median |
| Keystroke to composed screen, animating | 47 ms median | 12 ms median |
| Animation at the display's rate | 57.7–60 fps, timer drifting against refreshes | every refresh, none repeated or dropped |

`ZGUI_PRESENT_LOG=1` logs each shown present as DXGI reports it. The probes
measure the same things on any machine:

```powershell
cargo build --release -p zgui-desktop --example gallery --example input_latency --example animation_workload
python scripts/windows_idle_probe.py target/release/examples/gallery.exe
python scripts/windows_latency_probe.py target/release/examples/input_latency.exe --click 100,44 --region 24,24,520,40 --env ZGUI_ANIMATE=1
python scripts/windows_pacing_probe.py target/release/examples/animation_workload.exe
```

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
cargo build -p zgui-desktop --example gallery --example dpi_transition --example native_ime
python scripts/windows_desktop_smoke.py target/debug/examples/gallery.exe
python scripts/windows_dpi_smoke.py target/debug/examples/dpi_transition.exe
python scripts/windows_ime_smoke.py target/debug/examples/native_ime.exe --install-korean
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
