package workload

import (
	"fmt"
	"math"
	"strings"
	"sync"
	"time"
)

// The heavy dashboard's geometry and colours (the crate's `heavy`).
const (
	HeavyWidth, HeavyHeight = 1280.0, 800.0
	// Font is installed on every Mac, so no adapter falls back to another
	// face.
	Font          = "Arial"
	Background    = 0x0e1218
	Panel         = 0x161c26
	HeaderColor   = 0x1c2430
	Stripe        = 0x19202b
	Border        = 0x263041
	Track         = 0x243042
	TextColor     = 0xe5edf7
	Muted         = 0x8b98ab
	Accent        = 0x5eb1ff
	Title         = "Fleet overview"
	Tiles         = 6
	TileStep      = float32(208)
	CardColumns   = 6
	CardRows      = 5
	Cards         = CardColumns * CardRows
	Bars          = 26
	BarX          = float32(8)
	BarWidth      = float32(3)
	BarStep       = float32(4)
	SparkBase     = float32(96)
	SparkHeight   = float32(42)
	TableHeader   = float32(28)
	TableRow      = float32(24)
	TableRows     = 100_000
	TableOverscan = 2
	TableViewport = float32(552) - TableHeader
	CellTop       = float32(4)
	LogLines      = 5
	LogLineHeight = float32(16)
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
	Pill      = [4]float32{238, 4, 64, 16}
	PillText  = [2]float32{8, 1}
	LogOrigin = [2]float32{16, 684}
	Log       = [2]float32{1248, 100}
	LogTextAt = [2]float32{12, 8}

	tileLabels = [Tiles]string{"Requests / s", "Error rate", "p50 latency", "p99 latency", "Active workers", "Queue depth"}
	regions    = [4]string{"iad", "sfo", "fra", "nrt"}
)

// Heavy is the dashboard's state: live advances with stream and both,
// scroll with scroll and both.
type Heavy struct {
	Frames, Live uint64
	Scroll       float32
	Log          []string
	Mode         Mode
	Seconds      float64
}

// NewHeavy is a dashboard in mode with its first log lines.
func NewHeavy(mode Mode, seconds float64) *Heavy {
	h := &Heavy{Log: make([]string, 0, LogLines+1), Mode: mode, Seconds: seconds}
	for range LogLines {
		h.pushLog()
	}
	return h
}

// HeavyFromEnv is a dashboard as ZGX_MODE and ZGX_SECONDS ask.
func HeavyFromEnv() *Heavy { return NewHeavy(ModeFromEnv(), SecondsFromEnv()) }

// Turn reports whether the workload runs now: with ZGX_PARITY set to even
// or odd, only in Unix seconds of that parity, so two apps take turns
// (scripts/windowserver_pair.py); otherwise always.
func Turn() bool {
	p := parity()
	return p < 0 || time.Now().Unix()%2 == p
}

var parity = sync.OnceValue(func() int64 {
	switch Env("ZGX_PARITY") {
	case "even":
		return 0
	case "odd":
		return 1
	}
	return -1
})

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
		uint32(float32(Sample(node+600, live)*400)), status, uint32(live)*2_654_435_761)
	if len(h.Log) == LogLines {
		h.Log = h.Log[:copy(h.Log, h.Log[1:])]
	}
	h.Log = append(h.Log, line)
}

// VisibleLog is the log's lines.
func (h *Heavy) VisibleLog() string { return strings.Join(h.Log, "\n") }

// RowRange is the table rows to build.
func (h *Heavy) RowRange() (int, int) { return RowRangeAt(h.Scroll) }

// RowRangeAt is the table rows to build at scroll, [first, end), with
// TableOverscan rows each side.
func RowRangeAt(scroll float32) (int, int) {
	first := int(scroll / TableRow)
	end := int(math.Ceil(float64((scroll + TableViewport) / TableRow)))
	return max(first-TableOverscan, 0), min(end+TableOverscan, TableRows)
}

// Sample is a smooth pseudo-random series in [0.02, 1], one per series.
func Sample(series int, t uint64) float32 {
	x := float32(float32(t)*0.09) + float32(float32(series)*1.7)
	v := 0.5 + float32(0.28*Sinf(x)) + float32(0.14*Sinf(float32(x*2.3)+float32(series))) + float32(0.08*Sinf(float32(x*5.1)))
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
		return fmt.Sprintf("%.2f%%", float64(float32(3*s)))
	case 2:
		return fmt.Sprintf("%.1f ms", float64(4+float32(30*s)))
	case 3:
		return fmt.Sprintf("%.1f ms", float64(40+float32(200*s)))
	case 4:
		return fmt.Sprint(800 + uint32(float32(s*400)))
	}
	return fmt.Sprint(uint32(float32(s * 5_000)))
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

// BarHeight is bar bar's height: the series advances a sample a tick, so
// the sparkline scrolls left.
func BarHeight(card, bar int, live uint64) float32 {
	return max(SparkHeight*Sample(card, live+uint64(bar)), 2)
}

func BarXAt(bar int) float32 { return BarX + float32(float32(bar)*BarStep) }

func RowID(row int) string { return fmt.Sprintf("#%06d", row) }

func RowWorker(row int) string {
	return fmt.Sprintf("worker-%05d.%s", row%100_000, regions[row%4])
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
