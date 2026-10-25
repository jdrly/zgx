# Rune adapters

The comparison scenes in [Rune](https://github.com/anomalyco/rune) (MIT), a
Rust UI framework on wgpu and winit (Taffy layout, Parley text, a keyed
retained tree rebuilt from plain view functions), pinned at `dev`
**998bef587f09e359d877b35c66e48b8f355e73cb** with one local patch
(`PATCH.md`). They follow the contract in `../README.md`.

```sh
./build.sh                       # bin/{heavy,lab,animate,vlist}, build.json
ZGX_MODE=both ZGX_SECONDS=10 bin/heavy
```

`build.sh` checks Rune out into `rune-src/`, applies the patch, and builds
with Rune's own benchmark profile, `--profile perf` (release, opt-level 3,
16 codegen units, no LTO, not incremental), which its bench scripts require.
`Cargo.lock` starts from Rune's own, so the adapters build the dependency
versions Rune pins.

## How the scenes use Rune

- The window is a standard titled one (`Chrome { full_size_content: false }`)
  of the scene's size, not resizable (`WindowOptions { resizable: false }`).
- Translucency composites in sRGB (`App::srgb_blend(true)`, documented as
  matching CSS and Core Animation), as the other frameworks blend, rather
  than Rune's default linear light. It's a surface format and a shader flag,
  no extra pass.
- The font: the runner's `ZGX_FONT_FILE`, read once and given to
  `AppSpec::fonts` (which replaces Rune's built-in Inter, with no system
  fonts), named as both families. Reported at start, as every adapter does.
- Ticks: the app's `poll`, which Rune calls at each frame's start, with
  `Tx::wake_after` scheduling the next deadline (Rune runs no timer thread).
  Missed deadlines are dropped.
- `heavy` and `lab`: a tick changes the state and the view re-runs; Rune
  diffs it against its retained tree. Rune animates layout and colour changes
  on springs by default; a tick applies at once (`Tx::instant`), as live data
  does in the other frameworks. Their tables and lists are the same
  application-level fixed-height virtualization as the other adapters',
  rows keyed by their index. A Rune border takes no layout space, where
  CSS (which the scene's geometry follows, as zgx, GPUI and MyGo lay it out)
  places an absolute child inside its parent's border: heavy's panel
  children are placed 1 pt in, by the border's width.
- `animate`: the dots' and characters' opacities are Rune's high-frequency
  values (`Live<f32>` bound with `live_opacity`), set in `poll` at the
  frame's time, which asks for the next frame (`wake_after(0)`): the view
  doesn't re-run and nothing is laid out. The shimmer's characters are white
  text whose opacity is the scene's alpha.
- `vlist`: Rune's virtualized list, `El::virtual_rows` on a growth stack
  inside a scroll viewport. Scrolls go through `Tx::wheel` (the patch);
  jumps through `Tx::scroll_into_view`, which reveals the row as little as
  shows it (nearest), not at the top as the other adapters place it.

### How Rune's list measures heights

From `crates/rune/src/virtual_list.rs` and `virtual_rows.rs` at the pinned rev:

- **What it builds:** only the rows in its mounted window: the viewport and
  up to two viewports either side. `build(i)` runs for those rows only,
  after layout knows where the window is. Rows new to the window beyond the
  viewport mount half a viewport per side per frame.
- **Measured and estimated heights:** each row's height is measured as it is
  laid out and kept by the row's key while the key stays. A row not yet
  measured takes the list's estimate (here a three-line row without an
  image, as zgx's). Running totals of the heights give the window's position.
- **The anchor:** corrections from measurement keep the visible row in
  place, and the bottom exact. If estimates leave a gap, the measurement pass
  widens the window before drawing.

## The microbenchmarks

`SCENE=state|vscroll-fixed|vscroll-variable bin/micro` times
`zgx_workload::micro`'s operations in Rune's headless runtime, as
`micro_bench` does zgx's: process CPU time per operation, batched, for the
write alone and through to a frame ready to present. A write is
`Runtime::update` (state) or a 12 pt `Runtime::wheel_xy` step (the path a
wheel event takes). A frame is what Rune's own headless runner does for one:
`Runtime::frame` (commit, layout, the scene's instances), then the renderer's
prepare, draw and submit to an offscreen target at scale 2. The GPU settles
between batches, untimed. Its blending is Rune's default (the scenes are
opaque).

## The vlist probe

`ZGX_MODE=probe bin/vlist` runs in Rune's headless runtime
(`Runtime::with_text`, the same font): it reveals each probe row with
`scroll_into_view`, lets it lay out, and reads its drawn height
(`Runtime::presented`). A row's height doesn't depend on where it sits, so
it reports what the window would lay out.
