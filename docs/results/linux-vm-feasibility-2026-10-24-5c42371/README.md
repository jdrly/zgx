# Linux VM feasibility run, 2026-10-24 (zgx `5c42371`)

Can zgx and GPUI be compared on Linux in a VM on the Mac: CPU (from `/proc`)
and RSS on the heavy dashboard and the animation, with no Linux GPU? Yes,
cheaply, **with software rendering**, which is what these numbers are.

**Not GPU performance.** The VM has no GPU: both frameworks render through
Mesa's lavapipe (Vulkan on llvmpipe), so the CPU below includes rasterizing
every drawn pixel on the CPU. That grows with the pixels a frame draws, and
zgx's damage tracking draws far fewer, so its lead here (scroll, animate) is
larger than on a GPU would be. Use this run to check that the setup works and
to compare CPU side by side under software Vulkan, not as a hardware result.
It is not on the results site.

Setup (`environment.json`): an OrbStack VM, Ubuntu 24.04 arm64 with 4 vCPUs
and 16 GiB on an M5 Max shared with other work; Xvfb at 2560×1600 with
openbox; Mesa 25.2.8 lavapipe, `LP_NUM_THREADS=4`; zgx's `heavy_workload` and
`animation_workload` and GPUI 0.2.2's `heavy` and `animate`
(`comparisons/gpui`), all built with the `dist` profile (`build.sh`,
`build.log`). `measure.py` runs scripts/compare.py's `run_trial`: 20 s a
trial, 5 s warmup, process CPU (utime + stime) and RSS every 50 ms, three
repeats in rotating order (`measure.log`, `rows.json`, a JSON and log a
trial).

Median CPU, percent of one core, [range of the three repeats]:

| Scene, mode | zgx | GPUI |
| --- | ---: | ---: |
| heavy, idle | 0.0 [0.0–0.1] | 0.3 [0.1–0.3] |
| heavy, stream | 91.8 [80.1–117.4] | 174.1 [0.3–180.9] |
| heavy, scroll | 7.3 [7.2–10.2] | 192.8 [190.7–199.0] |
| heavy, both | 121.0 [114.3–145.6] | 174.5 [174.4–175.5] |
| animate | 5.7 [4.8–9.5] | 69.6 [57.2–74.6] |

Median RSS, MiB: heavy idle 149.5 / 157.6, stream 161.4 / 158.4, scroll
155.6 / 158.7, both 168.5 / 159.4; animate 140.9 / 141.8 (zgx / GPUI).

- GPUI does not keep up in heavy: 53.2 ticks a second streaming, 46.1
  scrolling and 53.6 both, of the 60 asked (zgx 59.8–60.0), so its CPU there
  understates what the scene asks of it. In animate both draw at about 60.
- One GPUI stream trial (`heavy.gpui.stream.1`) took 0.3% CPU over a full
  run of ticks: it seemingly stopped drawing. The median ignores it; a run to
  publish repeats it.
- zgx's heavy stream and both vary about ±20% between repeats: the VM shared
  the Mac's CPUs with other work.

The sampler counted the animation apps' `animation_frames=` as ticks with
the change to scripts/compare.py made alongside this run.
