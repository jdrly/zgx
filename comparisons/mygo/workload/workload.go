// Package workload is comparisons/workload (the zgx-workload crate) in Go:
// the same deterministic text, numbers and geometry, so the MyGo adapters
// show what the zgx and GPUI adapters show. testdata/golden.txt is the
// crate's output (its `golden` example); the test checks this package
// prints it byte for byte.
package workload

/*
#include <math.h>
*/
import "C"

import (
	"fmt"
	"math"
	"os"
	"strconv"
	"strings"
	"time"
)

const (
	Width          = 960.0
	Height         = 720.0
	RowHeight      = float32(28.0)
	RowCount       = 100_000
	ViewportHeight = float32(400.0)
	Period         = 16_666_667 * time.Nanosecond
	Token          = "The quick brown fox streams a token. "
)

// Mode is what a run advances: nothing, the text, the scroll, or both.
type Mode int

const (
	Idle Mode = iota
	Stream
	Scroll
	Both
)

// env is os.Getenv(key), or for one release the name it had before the
// rename to zgx.
func env(key string) string {
	if v, ok := os.LookupEnv(key); ok {
		return v
	}
	return os.Getenv(strings.Replace(key, "ZGX_", "ZGUI_", 1)) // rename-to-zgx: keep
}

// ModeFromEnv reads ZGX_MODE (both by default).
func ModeFromEnv() Mode {
	switch env("ZGX_MODE") {
	case "idle":
		return Idle
	case "stream":
		return Stream
	case "scroll":
		return Scroll
	}
	return Both
}

// SecondsFromEnv reads ZGX_SECONDS (30 by default).
func SecondsFromEnv() float64 {
	if v, err := strconv.ParseFloat(env("ZGX_SECONDS"), 64); err == nil && v >= 0 && !math.IsInf(v, 0) {
		return v
	}
	return 30
}

// sinf is libm's single-precision sine, the one Rust's f32::sin calls, so
// the series match the crate's to the bit.
func sinf(x float32) float32 { return float32(C.sinf(C.float(x))) }

// Workload is the lab scene's streaming text and scroll offset.
type Workload struct {
	Text         string
	ScrollOffset float32
	Frames       uint64
	Mode         Mode
	Seconds      float64
}

// FromEnv is a workload as ZGX_MODE and ZGX_SECONDS ask (ZGX_INITIAL_TICKS
// ticks of both first).
func FromEnv() *Workload {
	w := &Workload{Mode: ModeFromEnv(), Seconds: SecondsFromEnv()}
	initial, _ := strconv.ParseUint(env("ZGX_INITIAL_TICKS"), 10, 64)
	if initial > 100_000 {
		initial = 100_000
	}
	mode := w.Mode
	w.Mode = Both
	for i := uint64(0); i < initial; i++ {
		w.Tick()
	}
	w.Mode = mode
	return w
}

func (w *Workload) Tick() {
	w.Frames++
	if w.Mode == Stream || w.Mode == Both {
		w.Text += fmt.Sprintf("%06d %s", w.Frames, Token)
		if len(w.Text) > 8192 {
			w.Text = w.Text[len(w.Text)-8192:]
		}
	}
	if w.Mode == Scroll || w.Mode == Both {
		w.ScrollOffset = float32(math.Mod(float64(w.ScrollOffset+14), float64(float32(RowCount)*RowHeight-ViewportHeight)))
	}
}

// RowRange is the rows to build: those in view and two more each side.
func (w *Workload) RowRange() (int, int) {
	first := int(w.ScrollOffset / RowHeight)
	end := int(math.Ceil((float64(w.ScrollOffset) + float64(ViewportHeight)) / float64(RowHeight)))
	return max(first-2, 0), min(end+2, RowCount)
}

// VisibleText is the transcript's last 735 bytes in lines of 105.
func (w *Workload) VisibleText() string {
	tail := w.Text[max(len(w.Text)-735, 0):]
	var lines []string
	for len(tail) > 0 && len(lines) < 7 {
		n := min(105, len(tail))
		lines = append(lines, tail[:n])
		tail = tail[n:]
	}
	return strings.Join(lines, "\n")
}

func RowLabel(index int) string { return fmt.Sprintf("Row %06d — retained virtual item", index) }

// Animation is the animate scene's constants and curves.
var Animation = struct {
	Columns, Rows    int
	Dot, Gap         float32
	Origin           [2]float32
	Shimmer          string
	ShimmerOrigin    [2]float32
	DotColor, Background uint32
}{12, 4, 14, 12, [2]float32{40, 120}, "Reading workspace files and streaming tokens", [2]float32{40, 260}, 0x9ad2ff, 0x10141c}

func remEuclid(a, b float32) float32 {
	r := float32(math.Mod(float64(a), float64(b)))
	if r < 0 {
		r += b
	}
	return r
}

func DotPosition(index int) (float32, float32) {
	column, row := index%Animation.Columns, index/Animation.Columns
	return Animation.Origin[0] + float32(float32(column)*(Animation.Dot+Animation.Gap)), Animation.Origin[1] + float32(float32(row)*(Animation.Dot+Animation.Gap))
}

// DotOpacity is a 750 ms blink with a per-dot phase offset.
func DotOpacity(index int, seconds float32) float32 {
	phase := remEuclid(seconds/0.75-float32(float32(index)*0.07), 1)
	switch {
	case phase < 0.45:
		p := 1 - phase/0.45
		return 1 - float32(0.9*(1-float32(float32(p*p)*p)))
	case phase < 0.92:
		return 0.1
	}
	return 1
}

// ShimmerAlpha is white text's alpha under a band sweeping every 3.4 s.
func ShimmerAlpha(index, count int, seconds float32) uint8 {
	last := float32(max(count, 2) - 1)
	centre := -0.4 + float32(1.8*remEuclid(seconds/3.4, 1))
	x := 1 - float32(math.Abs(float64(float32(index)/last-centre)))/0.36
	x = min(max(x, 0), 1)
	return uint8(90 + float32(float32(float32(165*x)*x)*(3-float32(2*x))))
}

// The heavy dashboard's geometry and colours (see the crate's `heavy`).
const (
	HeavyWidth, HeavyHeight = 1280.0, 800.0
	Font                    = "Arial"
	Background              = 0x0e1218
	Panel                   = 0x161c26
	HeaderColor             = 0x1c2430
	Stripe                  = 0x19202b
	Border                  = 0x263041
	Track                   = 0x243042
	TextColor               = 0xe5edf7
	Muted                   = 0x8b98ab
	Accent                  = 0x5eb1ff
	Title                   = "Fleet overview"
	Tiles                   = 6
	TileStep                = 208.0
	CardColumns, CardRows   = 6, 5
	Cards                   = CardColumns * CardRows
	Bars                    = 26
	BarX, BarWidth, BarStep = 8.0, 3.0, 4.0
	SparkBase, SparkHeight  = 96.0, 42.0
	TableHeader             = 28.0
	TableRow                = float32(24.0)
	TableRows               = 100_000
	TableOverscan           = 2
	TableViewport           = float32(552.0 - TableHeader)
	CellTop                 = 4.0
	LogLines                = 5
	LogLineHeight           = 16.0
)

var (
	TitleOrigin = [2]float32{16, 14}
	Tile        = [2]float32{200, 60}
	TileOrigin  = [2]float32{16, 48}
	TileLabelAt = [2]float32{12, 8}
	TileValueAt = [2]float32{12, 26}
	Card        = [2]float32{120, 104}
	CardOrigin  = [2]float32{16, 120}
	CardStep    = [2]float32{128, 112}
	CardTitleAt = [2]float32{8, 6}
	CardValueAt = [2]float32{8, 20}
	Progress    = [4]float32{8, 46, 104, 4}
	TableOrigin = [2]float32{792, 120}
	Table       = [2]float32{472, 552}
	Columns     = [5]struct {
		X     float32
		Title string
	}{{10, "ID"}, {84, "Worker"}, {238, "Status"}, {312, "Latency"}, {386, "Throughput"}}
	Pill       = [4]float32{238, 4, 64, 16}
	PillText   = [2]float32{8, 1}
	LogOrigin  = [2]float32{16, 684}
	Log        = [2]float32{1248, 100}
	LogTextAt  = [2]float32{12, 8}
	tileLabels = [Tiles]string{"Requests / s", "Error rate", "p50 latency", "p99 latency", "Active workers", "Queue depth"}
)

// Heavy is the dashboard's state.
type Heavy struct {
	Frames, Live uint64
	Scroll       float32
	Log          []string
	Mode         Mode
	Seconds      float64
}

func HeavyFromEnv() *Heavy {
	h := &Heavy{Mode: ModeFromEnv(), Seconds: SecondsFromEnv()}
	for i := 0; i < LogLines; i++ {
		h.pushLog()
	}
	return h
}

func (h *Heavy) Tick() {
	h.Frames++
	if h.Mode == Stream || h.Mode == Both {
		h.Live++
		h.pushLog()
	}
	if h.Mode == Scroll || h.Mode == Both {
		h.Scroll = float32(math.Mod(float64(h.Scroll+14), float64(float32(TableRows)*TableRow-TableViewport)))
	}
}

func (h *Heavy) pushLog() {
	live, node := h.Live, int(h.Live%Cards)
	status, _ := Status(node, live)
	line := fmt.Sprintf("%06d  %s  latency %s  p99 %.1f ms  queue %3d  %s  checksum %08x",
		live, CardTitle(node), CardValue(node, live), float64(8+float32(180*Sample(node+300, live))),
		uint32(Sample(node+600, live)*400), status, uint32(live)*2_654_435_761)
	h.Log = append(h.Log, line)
	if len(h.Log) > LogLines {
		h.Log = h.Log[1:]
	}
}

func (h *Heavy) VisibleLog() string { return strings.Join(h.Log, "\n") }

func (h *Heavy) RowRange() (int, int) { return RowRangeAt(h.Scroll) }

// RowRangeAt is the table rows to build at `scroll`, with the overscan.
func RowRangeAt(scroll float32) (int, int) {
	first := int(scroll / TableRow)
	end := int(math.Ceil(float64((scroll + TableViewport) / TableRow)))
	return max(first-TableOverscan, 0), min(end+TableOverscan, TableRows)
}

// Sample is a smooth pseudo-random series in [0.02, 1], one per `series`.
func Sample(series int, t uint64) float32 {
	var x float32
	// Every product rounded on its own (a conversion stops Go fusing a
	// multiply into the add), as Rust computes it.
	x = float32(float32(t)*0.09) + float32(float32(series)*1.7)
	v := 0.5 + float32(0.28*sinf(x)) + float32(0.14*sinf(float32(x*2.3)+float32(series))) + float32(0.08*sinf(float32(x*5.1)))
	return min(max(v, 0.02), 1)
}

func TileLabel(tile int) string { return tileLabels[tile] }

func TilePosition(tile int) (float32, float32) {
	return TileOrigin[0] + float32(float32(tile)*TileStep), TileOrigin[1]
}

func TileValue(tile int, live uint64) string {
	s := Sample(900+tile, live)
	switch tile {
	case 0:
		return fmt.Sprintf("%.0f", float64(12_000+float32(40_000*s)))
	case 1:
		return fmt.Sprintf("%.2f%%", float64(3*s))
	case 2:
		return fmt.Sprintf("%.1f ms", float64(4+float32(30*s)))
	case 3:
		return fmt.Sprintf("%.1f ms", float64(40+float32(200*s)))
	case 4:
		return fmt.Sprint(800 + uint32(s*400))
	}
	return fmt.Sprint(uint32(s * 5_000))
}

func CardPosition(card int) (float32, float32) {
	column, row := card%CardColumns, card/CardColumns
	return CardOrigin[0] + float32(float32(column)*CardStep[0]), CardOrigin[1] + float32(float32(row)*CardStep[1])
}

func CardTitle(card int) string { return fmt.Sprintf("node-%02d", card) }

func CardValue(card int, live uint64) string {
	return fmt.Sprintf("%.1f ms", float64(4+float32(96*Sample(card, live))))
}

// CardProgress is the progress fill's width.
func CardProgress(card int, live uint64) float32 { return Progress[2] * Sample(card+1000, live) }

// BarHeight is bar `bar`'s height: the series advances a sample a tick.
func BarHeight(card, bar int, live uint64) float32 {
	return max(SparkHeight*Sample(card, live+uint64(bar)), 2)
}

func BarXAt(bar int) float32 { return BarX + float32(float32(bar)*BarStep) }

func RowID(row int) string { return fmt.Sprintf("#%06d", row) }

func RowWorker(row int) string {
	return fmt.Sprintf("worker-%05d.%s", row%100_000, [4]string{"iad", "sfo", "fra", "nrt"}[row%4])
}

// Status is a status label and its pill colour.
func Status(series int, live uint64) (string, uint32) {
	s := Sample(series+5000, live/20)
	switch {
	case s > 0.8:
		return "degraded", 0x8a5a12
	case s < 0.12:
		return "down", 0x8a2230
	}
	return "healthy", 0x1d6b4a
}

func RowLatency(row int, live uint64) string {
	return fmt.Sprintf("%.1f ms", float64(2+float32(120*Sample(row+2000, live))))
}

func RowThroughput(row int, live uint64) string {
	return fmt.Sprintf("%.2f MB/s", float64(0.2+float32(48*Sample(row+3000, live))))
}

func RowColor(row int) uint32 {
	if row%2 == 0 {
		return Panel
	}
	return Stripe
}
