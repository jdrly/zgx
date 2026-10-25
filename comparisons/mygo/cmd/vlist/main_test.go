package main

import (
	"image/color"
	"testing"

	"github.com/egoist/mygo/ui"
	w "github.com/zeronsh/zgx/comparisons/mygo/workload"
)

// run ticks the scene in a tester, a frame after each tick that moved it.
func run(t *testing.T, mode w.VMode, ticks int, each func(tt *ui.Tester, s *chat)) {
	s := &chat{mode: mode, jump: -1}
	tt := ui.NewTester(s.view, int(w.VWidth), int(w.VHeight))
	for range ticks {
		if s.tick() {
			tt.Frame()
		}
		each(tt, s)
	}
}

// blank is the probe's test: a run over 24 DIPs down a column near the left
// edge whose pixels are closer to the window's background than to the rows'.
func blank(tt *ui.Tester) bool {
	img := tt.Image()
	scale := float32(img.Bounds().Dx()) / w.VWidth
	dist := func(c color.RGBA, rgb uint32) int {
		d := func(a uint8, b uint32) int { x := int(a) - int(b&0xff); return x * x }
		return d(c.R, rgb>>16) + d(c.G, rgb>>8) + d(c.B, rgb)
	}
	for _, x := range []float32{4, 8, 12} {
		run := 0
		for y := 0; y < img.Bounds().Dy(); y++ {
			c := img.RGBAAt(int(x*scale), y)
			bg := dist(c, w.VBackground)
			if bg < dist(c, w.RowColors[0]) && bg < dist(c, w.RowColors[1]) {
				if run++; float32(run) > 24*scale {
					return true
				}
			} else {
				run = 0
			}
		}
	}
	return false
}

func TestJumpsLandOnTheirRowAndScrollOnFromThere(t *testing.T) {
	run(t, w.VJump, 3*w.JumpEvery+2, func(tt *ui.Tester, s *chat) {
		first, _ := s.list.Visible()
		target, jumped := w.Jump(s.ticks)
		switch {
		case jumped && first != target:
			t.Fatalf("tick %d jumped to row %d, want %d", s.ticks, first, target)
		case s.ticks > w.JumpEvery && s.ticks%w.JumpEvery == 1:
			// The tick after a jump scrolls 12 DIPs on from its row.
			if want, _ := w.Jump(s.ticks - 1); first != want {
				t.Fatalf("tick %d left the jump's row %d for %d", s.ticks, want, first)
			}
		}
		if blank(tt) {
			t.Fatalf("tick %d shows blank rows", s.ticks)
		}
	})
}

func TestScrollMovesEveryTickWithoutBlankRows(t *testing.T) {
	var last float32 = -1
	run(t, w.VScroll, 200, func(tt *ui.Tester, s *chat) {
		if s.scroll.Y <= last {
			t.Fatalf("tick %d: offset %v after %v", s.ticks, s.scroll.Y, last)
		}
		last = s.scroll.Y
		if blank(tt) {
			t.Fatalf("tick %d shows blank rows", s.ticks)
		}
	})
}
