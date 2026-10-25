package workload

import (
	"strings"
	"testing"
)

// The crate's own tests, ported.

func TestVisibleRangeUsesTwoRowsOfOverscanOnEachSide(t *testing.T) {
	w := New(Idle, 1)
	if a, b := w.RowRange(); a != 0 || b != 17 {
		t.Fatalf("%d..%d", a, b)
	}
	w.ScrollOffset = 280
	if a, b := w.RowRange(); a != 8 || b != 27 {
		t.Fatalf("%d..%d", a, b)
	}
	w.ScrollOffset = float32(RowCount)*RowHeight - ViewportHeight
	if a, b := w.RowRange(); b != RowCount || b-a > 19 {
		t.Fatalf("%d..%d", a, b)
	}
}

func TestBoundedAndDeterministic(t *testing.T) {
	w := New(Both, 1)
	for range 10_000 {
		before := w.VisibleText()
		w.Tick()
		if a, b := w.RowRange(); len(w.text) > 8192 || b-a > 19 || before == w.VisibleText() {
			t.Fatal(len(w.text), a, b)
		}
	}
	if w.ScrollOffset != 140_000 {
		t.Fatal(w.ScrollOffset)
	}
}

func TestHeavyGeometryFitsTheWindowAndTheSeriesIsBounded(t *testing.T) {
	x, y := CardPosition(Cards - 1)
	if x+Card[0] > TableOrigin[0] || y+Card[1] > LogOrigin[1] || BarXAt(Bars-1)+BarWidth > Card[0]-BarX {
		t.Fatal("cards overflow")
	}
	for i := range uint64(5_000) {
		if s := Sample(int(i%97), i); s < 0.02 || s > 1 {
			t.Fatal(i, s)
		}
	}
	if a, b := RowRangeAt(0); b-a != 24 {
		t.Fatal(a, b)
	}
}

func TestHeavyModesAdvanceOnlyTheirParts(t *testing.T) {
	h := NewHeavy(Scroll, 1)
	h.Tick()
	if h.Live != 0 || h.Scroll != 14 {
		t.Fatal(h.Live, h.Scroll)
	}
	h.Mode = Stream
	before := h.VisibleLog()
	h.Tick()
	if h.Live != 1 || h.Scroll != 14 || before == h.VisibleLog() || len(h.Log) != LogLines {
		t.Fatal(h.Live, h.Scroll, len(h.Log))
	}
}

func TestVlistRowsAreDeterministicAndVaried(t *testing.T) {
	var counts [9]int
	images := 0
	for r := range 1000 {
		counts[Lines(r)]++
		if HasImage(r) {
			images++
		}
	}
	for _, n := range counts[1:] {
		if n <= 80 {
			t.Fatal(counts)
		}
	}
	if images < 200 || images >= 300 || Text(42) != Text(42) || len(strings.Split(Text(7), " ")) != Lines(7)*wordsPerLine {
		t.Fatal(images)
	}
}

func TestVlistModesMoveAsDescribed(t *testing.T) {
	if Delta(VScroll, 5) != ScrollStep || Delta(VFling, 0) <= Delta(VFling, 30) || Delta(VFling, Fling.Every) != Fling.Velocity {
		t.Fatal("deltas")
	}
	if _, ok := Jump(0); ok {
		t.Fatal("jump at 0")
	}
	if r, ok := Jump(JumpEvery); !ok || r >= VRows {
		t.Fatal(r, ok)
	}
	if _, ok := Jump(JumpEvery + 1); ok {
		t.Fatal("jump off its tick")
	}
}
