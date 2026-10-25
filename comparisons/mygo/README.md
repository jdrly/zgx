# MyGo adapters

The comparison scenes in [MyGo](https://github.com/egoist/mygo) (MIT),
pinned at **v0.6.3** (commit `15242900157b77228d35ca071f43e94a11bc90a0`).
They use its native GPU UI (package `ui`, drawn with Metal) and never its
webview. They follow the contract in `../README.md`: the same environment,
ticks, window sizes and report lines as the zgx and GPUI Fast adapters.

```sh
./build.sh                       # bin/{heavy,lab,animate,vlist}, build.json
ZGX_MODE=both ZGX_SECONDS=10 bin/heavy
```

`build.sh` builds as a MyGo app ships: `go build -trimpath -ldflags="-s
-w"`, without the race detector and with `CGO_ENABLED=0`, as MyGo itself
builds (it is pure Go, calling the system through purego). `build.json`
records the Go version, the flags, and MyGo's version, commit and license.

## The workload in Go

`workload` is `../workload` (the zgx-workload crate) ported to Go.
`go test ./workload` checks that it prints the crate's golden dump
(`cargo run -p zgx-workload --example golden`, kept in
`workload/testdata/golden.txt`) byte for byte. Two things make that hold:

- Float arithmetic is float32, as the crate's is, and each product is
  converted to float32 on its own. Without that, Go on arm64 fuses a
  multiply into the add after it, which Rust never does.
- Rust's `f32::sin` calls macOS libm's `sinf`, which is not correctly
  rounded: `float32(math.Sin(x))` differs from it for 1.8% of inputs, and
  some of those differences show in the scenes' text. Calling libm (cgo or
  purego) costs about 140 ns a call against 3 ns, and the heavy dashboard
  takes about 2,900 sines a tick. So `workload/sinf.go` is Apple's `sinf`
  (libsystem_m, macOS 26, arm64) ported instruction for instruction, fused
  where it fuses. It is checked bit-identical to libm for every float32:
  `go test ./workload -run SinfIsLibm -libm.all` (about 17 s; without the
  flag, every 61st input). Only this check uses cgo.

## Scenes

- `heavy`: the dashboard, built anew for each frame after a tick, as MyGo
  views are. The table shows the rows in view and two more each side,
  placed by the scroll offset, as the GPUI Fast adapter does.
- `lab`: the performance lab. Its list is the same application-level
  fixed-height virtualization as the other adapters', which the wheel
  scrolls too.
- `animate`: the view asks for every display frame (`AnimationFrame`),
  its clock at the frame's time (`Context.Now`). It counts frames by
  their time, since one frame can build the view more than once. The
  shimmer is one `ui.RichText` with a span for each character, where the
  zgx and GPUI adapters make a text node for each character in a row. In
  MyGo a text of a lone space has no width (as CSS drops a white-space
  flex item), so per-character texts run the words together.
- `vlist`: MyGo's own virtualized list, `ui.List` with a `ListState`.
  Scrolls (all three modes; jump scrolls between its jumps) set its offset
  through a `ScrollState` (`TrackScroll`), as an app scrolling a list by
  pixels does. A jump calls `ListState.ScrollTo(row, ui.Start)`. At the
  end, the scroll wraps back to the top.

Every scene draws its text in the one font file of `../fonts`, which the
runner passes as `ZGX_FONT_FILE`: the adapters register it with
`ui.RegisterFont` as the family `DejaVu Sans`, the only family their text
names, and print `font_family=DejaVu Sans font_file_fnv64=…` at start.

`ZGX_MODE=probe` runs vlist's probe instead of a trial: each of
`ProbeRows` is put at the top (`ScrollTo`), given four frames, and the
height the list laid it out at (`Element.LayoutBounds`) is kept. Then it
prints `row_heights=<row>:<h>,… list_width=… text_width=688.0` and exits.

Every adapter asks for a frame only when a tick changed what it shows
(`Window.Invalidate`): idle draws nothing after its first frames, and a
vlist tick that moves nothing draws nothing.

## The microbenchmarks

`SCENE=state|vscroll-fixed|vscroll-variable ZGX_FONT_FILE=… GOMAXPROCS=1
bin/micro` times the workload's micro scenes (`workload/micro.go`, the
crate's `micro`) in MyGo's test host, `ui.Tester`, as zgx's `micro_bench`
does: process CPU time per operation, batched, for the write alone and
through to a frame. A write is what a windowed app does: a store, plus
`Window.Invalidate`'s atomic flag (a counter, or the list's `ScrollState`
12 pt on). A tester frame builds, lays out and paints into its scene, then
rasterizes what changed on the CPU, which a window does on the GPU, and the
public API can't stop before the raster. So each frame figure comes twice:
`"raster":"1x"` and `"raster":"0.05x"` (layout and paint the same, the raster
almost no pixels: the closer match to zgx's frame). `GOMAXPROCS=1` is MyGo's
own published method, and the JSON records it.

### How `ui.List` measures heights

From `ui/list.go` at v0.6.3:

- **Its place is a row, not an offset.** The list keeps an anchor row and
  how far it is scrolled past that row's top. Rows measured above the
  anchor, or growing, don't move what is in view.
- **What it builds.** In the view, the list builds the rows its heights
  put in view, plus two on each side (`listOverscan`). As it lays out, it
  measures each built row at the list's content width with unbounded
  height. If the measured rows don't cover the viewport, it builds and
  measures more during the same layout (`buildLate`), so a frame shows no
  gap.
- **What it estimates.** Measured heights are kept per row, in sparse
  blocks of 64. A row not yet measured is estimated at the mean of the
  rows measured so far. Before any row is measured, the estimate is the
  theme's `Space(8)`. A change of width forgets every height.
- **Scrolling.** A scroll of up to two viewports moves the place by
  exactly as much, from the anchor. A longer one, or a `ScrollTo`, goes to
  where the known and estimated heights put that offset or row.

So, as with zgx's measured virtual list and GPUI's `list`, the scroll range
(`ScrollState.MaxY`) is an estimate that firms up as rows are measured.
