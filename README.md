# zgui

zgui is a retained-mode GUI framework in Rust for macOS, Windows and Linux. Components mount once and fine-grained signals update only the nodes that depend on them, so an idle window does no work and a busy one repaints only what changed. Interfaces are written in Rust or in Luau scripts that hot reload without losing state. In the [macOS benchmarks](docs/results/latest-macos-heavy-2026-10-11-5493829/) it uses less CPU and GPU time than GPUI in every scene measured, with similar memory.

- **Crates:** `zgui` (reactive core, layout, text, scene), `zgui-gpu` (wgpu renderer: Metal, Direct3D 12, Vulkan), `zgui-desktop` (winit host with native menus, dialogs, clipboard, IME, accessibility and multiple windows), `zgui-script` (Luau scripting).
- **Layout and paint:** flex and grid layout through Taffy, shaped rich text with fallbacks, virtual lists, selectable text, SVG, images, gradients, shadows, backdrop blur and damage-tracked rendering.
- **Scripting:** each script runs in its own sandboxed VM with heap and CPU limits. Release builds compile scripts to bytecode inside the binary. Plugins only get the capabilities the app grants them, and `zgui check` type-checks scripts with luau-lsp.

```sh
cargo run --release -p zgui-desktop --example gallery
cargo run --release -p zgui-script --example hot_reload
cargo run -p zgui-script --bin zgui -- check app.luau
```

On Linux, install the fontconfig, xkbcommon and D-Bus development packages and `pkg-config`. On Windows, use the MSVC toolchain ([notes](docs/windows.md)).

Docs: [components](docs/composition.md), [styling](docs/styling.md), [layout](docs/layout.md), [scripting](docs/scripting.md), [Luau reference](docs/reference/index.md), [benchmarking](docs/benchmarking.md), [roadmap](docs/ROADMAP.md). MIT licensed.
