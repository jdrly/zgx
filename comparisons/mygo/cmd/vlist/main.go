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
	// probe is the probe's progress (ZGX_MODE=probe), or nil.
	probe *probe
}

// probe shows each of rows at the top in turn and keeps the height the
// list laid it out at, a few frames after it came into view.
type probe struct {
	rows    []int
	heights []float32
	// at is the row being measured, frames the frames since it was asked
	// for, and height its laid-out height in the last frame.
	at, frames int
	height     float32
	width      float32
}

// probeFrames is the frames a row is given to be laid out and measured.
const probeFrames = 4

func (s *chat) view(c *ui.Context) {
	if s.probe != nil {
		s.probeStep(c)
	}
	if s.jump >= 0 {
		s.list.ScrollTo(s.jump, ui.Start)
		s.jump = -1
	}
	ui.Box(c).Fill().Background(adapter.RGB(w.VBackground)).Font(w.Font).FontSize(w.VFontSize).
		FixedLineHeight(w.VLineHeight).TextColor(adapter.RGB(w.VText)).Children(func() {
		list := ui.List(c, &s.list, w.VRows, func(i int) {
			e := adapter.ChatRow(c, i)
			if p := s.probe; p != nil && p.at < len(p.rows) && i == p.rows[p.at] {
				p.height = e.LayoutBounds().H
			}
		}).Fill().TrackScroll(&s.scroll)
		if s.probe != nil {
			s.probe.width = list.LayoutBounds().W
		}
	})
}

// probeStep asks for the probe's next row, keeps the height of the one
// laid out, and when all are measured reports them and quits.
func (s *chat) probeStep(c *ui.Context) {
	p := s.probe
	if p.at == len(p.rows) {
		return
	}
	switch p.frames++; {
	case p.frames == 1:
		s.jump = p.rows[p.at]
	case p.frames > probeFrames:
		p.heights = append(p.heights, p.height)
		p.at, p.frames, p.height = p.at+1, 0, 0
		if p.at == len(p.rows) {
			fmt.Fprintln(os.Stderr, w.ProbeReport(p.rows, p.heights, p.width))
			mygo.App.Quit()
			return
		}
	}
	c.Invalidate()
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
	if s.mode == w.VProbe {
		s.probe = &probe{rows: w.ProbeRows()}
	}
	adapter.Window("MyGo vlist", float64(w.VWidth), float64(w.VHeight), s.view, func(win *mygo.Window) {
		adapter.Ticks(win, w.SecondsFromEnv(), s.mode != w.VProbe, nil, s.tick, func() {
			fmt.Fprintf(os.Stderr, "workload_ticks=%d\n", s.ticks)
		})
	})
}
