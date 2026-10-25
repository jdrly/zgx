// Micro times the microbenchmarks (the workload's micro.go) in MyGo's own
// test host, ui.Tester: CPU time per operation for the operation alone
// ("write") and through to a frame ("frame"), as zgx's micro_bench does.
//
//		SCENE=state|vscroll-fixed|vscroll-variable ZGX_FONT_FILE=../fonts/DejaVuSans.ttf GOMAXPROCS=1 bin/micro
//
//	  - state: one text bound to a counter over a static grid. A write is what
//	    a windowed app does to change it: a store, and Window.Invalidate's
//	    atomic flag (not the main-thread post it makes once a frame).
//	  - vscroll-fixed, vscroll-variable: 10,000 rows in a ui.List, fixed or
//	    the vlist rows, measured as they mount. A write is a 12 pt step of the
//	    list's ScrollState (TrackScroll), back to the top past the end.
//
// A Tester frame builds, lays out and paints the view into its scene, then
// rasterizes what changed on the CPU, which a window does on the GPU; the
// public API can't stop before the raster. So each frame figure comes twice:
// "raster":"1x", the tester at scale 1, and "raster":"0.05x", at scale 0.05,
// where layout and paint are the same and the raster almost no pixels (the
// closer match to a frame ready to present on the GPU).
//
// CPU time is the process's (the garbage collector's work counts), read
// with clock_gettime_nsec_np(CLOCK_PROCESS_CPUTIME_ID), per batch, less the
// clock's own cost. One JSON line per figure on stdout.
package main

import (
	"fmt"
	"os"
	"runtime"
	"slices"
	"strconv"
	"sync/atomic"

	"github.com/ebitengine/purego"
	"github.com/egoist/mygo/ui"
	"github.com/zeronsh/zgx/comparisons/mygo/internal/adapter"
	w "github.com/zeronsh/zgx/comparisons/mygo/workload"
)

// clockProcessCPU is CLOCK_PROCESS_CPUTIME_ID on macOS.
const clockProcessCPU = 12

var cpuNanos func(clock uint32) uint64

func init() {
	lib, err := purego.Dlopen("/usr/lib/libSystem.B.dylib", purego.RTLD_NOW|purego.RTLD_GLOBAL)
	if err != nil {
		panic(err)
	}
	purego.RegisterLibFunc(&cpuNanos, lib, "clock_gettime_nsec_np")
}

func env(name string, def int) int {
	if v, err := strconv.Atoi(os.Getenv(name)); err == nil {
		return v
	}
	return def
}

// clockCost is the median CPU time of reading the clock once.
func clockCost() float64 {
	d := make([]uint64, 10_000)
	for i := range d {
		a := cpuNanos(clockProcessCPU)
		d[i] = cpuNanos(clockProcessCPU) - a
	}
	slices.Sort(d)
	return float64(d[len(d)/2])
}

// bench is one scene in a tester: what an operation writes, and its frames.
type bench struct {
	tt    *ui.Tester
	write func(op int)
}

// counter is the state scene.
type counter struct {
	n     uint64
	cells []string
	// invalidating is Window.Invalidate's flag, set by a write and cleared
	// as the frame it asked for builds.
	invalidating atomic.Bool
}

func (s *counter) view(c *ui.Context) {
	s.invalidating.Store(false)
	ui.Column(c).Fill().Padding(8).Gap(6).Background(adapter.RGB(w.VBackground)).Font(w.Font).
		FontSize(w.MicroFontSize).TextColor(adapter.RGB(w.VText)).Children(func() {
		ui.Text(c, w.CounterLabel(s.n)).Height(20)
		ui.Column(c).Gap(2).Children(func() {
			for r := range w.StaticRows {
				ui.Row(c).Gap(2).Children(func() {
					for col := range w.StaticColumns {
						ui.Text(c, s.cells[r*w.StaticColumns+col]).Size(w.Cell[0], w.Cell[1])
					}
				})
			}
		})
	})
}

// list is a vscroll scene.
type list struct {
	fixed        bool
	state        ui.ListState
	scroll       ui.ScrollState
	invalidating atomic.Bool
}

func (s *list) view(c *ui.Context) {
	s.invalidating.Store(false)
	ui.Box(c).Fill().Background(adapter.RGB(w.VBackground)).Font(w.Font).FontSize(w.VFontSize).
		FixedLineHeight(w.VLineHeight).TextColor(adapter.RGB(w.VText)).Children(func() {
		ui.List(c, &s.state, w.MicroRows, func(i int) {
			if !s.fixed {
				adapter.ChatRow(c, i)
				return
			}
			ui.Row(c).Height(w.FixedRow).PaddingX(12).AlignItems(ui.Center).Background(adapter.RGB(w.RowColors[i%2])).Children(func() {
				ui.Text(c, w.FixedLabel(i))
			})
		}).Fill().TrackScroll(&s.scroll)
	})
}

func newBench(scene string, scale float32) *bench {
	var b bench
	switch scene {
	case "state":
		s := &counter{}
		for i := range w.StaticRows * w.StaticColumns {
			s.cells = append(s.cells, w.StaticLabel(i))
		}
		b.tt = ui.NewTester(s.view, w.Width, w.Height)
		b.write = func(op int) {
			s.n = uint64(op + 1)
			s.invalidating.CompareAndSwap(false, true)
		}
	case "vscroll-fixed", "vscroll-variable":
		s := &list{fixed: scene == "vscroll-fixed"}
		b.tt = ui.NewTester(s.view, int(w.VWidth), int(w.MicroViewport))
		b.write = func(int) {
			// Past the end (as far as the rows' heights, measured or
			// estimated, reach), back to the top.
			y := s.scroll.Y + w.MicroStep
			if s.scroll.MaxY > 0 && y > s.scroll.MaxY {
				y = 0
			}
			s.scroll.Y = y
			s.invalidating.CompareAndSwap(false, true)
		}
	default:
		fmt.Fprintln(os.Stderr, "SCENE: state, vscroll-fixed or vscroll-variable")
		os.Exit(2)
	}
	b.tt.SetScale(scale)
	b.tt.Frame()
	return &b
}

// measure times ops operations in batches (after the warm-up), each a
// write and, for frame, the frame after it.
func (b *bench) measure(ops, batch int, frame bool, next *int) []float64 {
	one := func() {
		b.write(*next)
		*next++
		if frame {
			b.tt.Frame()
		}
	}
	for range env("WARMUP", w.MicroWarmup) {
		one()
	}
	cost := clockCost()
	samples := make([]float64, 0, ops/batch)
	for range ops / batch {
		start := cpuNanos(clockProcessCPU)
		for range batch {
			one()
		}
		samples = append(samples, max(float64(cpuNanos(clockProcessCPU)-start)-cost, 0)/float64(batch))
	}
	// A write alone leaves the frame to do: catch up, untimed.
	if !frame {
		b.tt.Frame()
	}
	return samples
}

func report(scene, figure, raster string, ops, batch int, s []float64) {
	slices.Sort(s)
	at := func(q float64) float64 { return s[int(float64(len(s)-1)*q+0.5)] }
	fmt.Printf(`{"framework":"mygo","scene":%q,"figure":%q,"raster":%q,"clock":"process_cpu","gomaxprocs":%d,"ops":%d,"batch":%d,"median_ns":%.1f,"p10_ns":%.1f,"p90_ns":%.1f,"min_ns":%.1f,"max_ns":%.1f,"batches":%d}`+"\n",
		scene, figure, raster, runtime.GOMAXPROCS(0), ops, batch, at(0.5), at(0.1), at(0.9), s[0], s[len(s)-1], len(s))
}

func main() {
	// The comparisons' font file, as every framework's micro shapes it (Go
	// can't embed it from outside this module).
	font := w.FontFile()
	if font == nil {
		fmt.Fprintln(os.Stderr, "ZGX_FONT_FILE: the comparisons' font file (comparisons/fonts/DejaVuSans.ttf)")
		os.Exit(2)
	}
	if err := ui.RegisterFont(font, w.Font); err != nil {
		panic(err)
	}
	scene := os.Getenv("SCENE")
	if scene == "" {
		scene = "state"
	}
	ops := env("OPS", w.MicroOps)
	writeBatch, frameBatch := env("WRITE_BATCH", 50), env("FRAME_BATCH", 10)
	b := newBench(scene, 1)
	next := 0
	// The write alone (a store and a flag, whatever the scale), then write
	// and frame at each raster scale.
	report(scene, "write", "1x", ops, writeBatch, b.measure(ops, writeBatch, false, &next))
	report(scene, "frame", "1x", ops, frameBatch, b.measure(ops, frameBatch, true, &next))
	small := newBench(scene, 0.05)
	next = 0
	report(scene, "frame", "0.05x", ops, frameBatch, small.measure(ops, frameBatch, true, &next))
}
