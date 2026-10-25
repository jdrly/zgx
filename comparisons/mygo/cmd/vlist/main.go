// Vlist is the variable-height list scene (see the workload's vlist.go) in
// MyGo's own virtualized list, ui.List: 100,000 chat rows of wrapped text,
// which the list measures as it lays them out and estimates beyond, moved
// each tick by the scene's scroll, fling or jump (a scroll with a jump every
// 30 ticks). Scrolls set the list's offset through a ScrollState
// (TrackScroll), as an app scrolling it by pixels does; a jump shows its
// row at the top with ListState.ScrollTo. A tick that moves nothing draws
// nothing.
//
//	ZGX_MODE=fling ZGX_SECONDS=10 bin/vlist
package main

import (
	"fmt"
	"os"

	"github.com/egoist/mygo"
	"github.com/egoist/mygo/ui"
	"github.com/zeronsh/zgx/comparisons/mygo/internal/adapter"
	w "github.com/zeronsh/zgx/comparisons/mygo/workload"
)

type chat struct {
	mode   w.VMode
	ticks  uint64
	list   ui.ListState
	scroll ui.ScrollState
	// jump is the row the last tick jumped to, for the view to scroll to,
	// or -1.
	jump int
}

func (s *chat) view(c *ui.Context) {
	if s.jump >= 0 {
		s.list.ScrollTo(s.jump, ui.Start)
		s.jump = -1
	}
	ui.Box(c).Fill().Background(adapter.RGB(w.VBackground)).Font(w.Font).FontSize(w.VFontSize).
		FixedLineHeight(w.VLineHeight).TextColor(adapter.RGB(w.VText)).Children(func() {
		ui.List(c, &s.list, w.VRows, func(i int) { row(c, i) }).Fill().TrackScroll(&s.scroll)
	})
}

func row(c *ui.Context, i int) {
	ui.Column(c).PaddingX(w.VPad[0]).PaddingY(w.VPad[1]).Gap(w.VGap).Background(adapter.RGB(w.RowColors[i%2])).Children(func() {
		ui.Row(c).Gap(8).Height(w.VLineHeight).Children(func() {
			ui.Text(c, w.Author(i)).TextColor(adapter.RGB(w.VAuthor))
			ui.Text(c, w.Time(i)).TextColor(adapter.RGB(w.VMuted))
		})
		ui.Text(c, w.Text(i)).Width(w.VTextWidth)
		if w.HasImage(i) {
			ui.Box(c).Size(w.VImage[0], w.VImage[1]).Background(adapter.RGB(w.VImageColor))
		}
	})
}

// tick moves the list, reporting whether it moved.
func (s *chat) tick() bool {
	s.ticks++
	if row, ok := w.Jump(s.ticks); ok && s.mode == w.VJump {
		s.jump = row
		return true
	}
	// At the end, back to the top (100k rows take minutes to reach).
	moved := s.scroll.Y + w.Delta(s.mode, s.ticks)
	if s.scroll.MaxY > 0 && moved > s.scroll.MaxY {
		moved = 0
	}
	if moved == s.scroll.Y {
		return false
	}
	s.scroll.Y = moved
	return true
}

func main() {
	s := &chat{mode: w.VModeFromEnv(), jump: -1}
	adapter.Window("MyGo vlist", float64(w.VWidth), float64(w.VHeight), s.view, func(win *mygo.Window) {
		adapter.Ticks(win, w.SecondsFromEnv(), true, nil, s.tick, func() {
			fmt.Fprintf(os.Stderr, "workload_ticks=%d\n", s.ticks)
		})
	})
}
