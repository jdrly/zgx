// Lab is the performance lab (see the workload's workload.go) in MyGo's
// native UI, as the zgx and GPUI Fast adapters draw it: a header, three
// buttons over a list of capacity four, the streaming pane and the rows in
// view of a 100,000-row list (and two each side), which the wheel scrolls
// too.
//
//	ZGX_MODE=both ZGX_SECONDS=10 bin/lab
package main

import (
	"fmt"
	"os"
	"strconv"

	"github.com/egoist/mygo"
	"github.com/egoist/mygo/ui"
	"github.com/zeronsh/zgx/comparisons/mygo/internal/adapter"
	w "github.com/zeronsh/zgx/comparisons/mygo/workload"
)

const (
	surface = 0x192230
	stripe  = 0x1d2838
	button  = 0x293b50
)

func at(e ui.Element, x, y, width, height float32) ui.Element {
	return e.Absolute().Left(x).Top(y).Size(width, height)
}

type lab struct {
	work  *w.Workload
	items []int
	done  int
}

func (l *lab) view(c *ui.Context) {
	ui.Box(c).Fill().Background(adapter.RGB(0x10141c)).TextColor(adapter.RGB(0xe5edf7)).
		Font("DejaVu Sans").FontSize(14).FixedLineHeight(20).Children(func() {
		at(ui.Text(c, "zgx performance lab").FontSize(20).FixedLineHeight(28), 20, 20, 920, 28)
		for _, b := range []struct {
			label  string
			x      float32
			action func()
		}{{"+", 20, l.append}, {"=", 80, l.write}, {strconv.Itoa(l.done), 140, func() { l.done = 999 }}} {
			at(ui.Box(c), b.x, 55, 50, 32).Background(adapter.RGB(button)).OnClick(b.action).Children(func() {
				at(ui.Text(c, b.label), 12, 6, 38, 20)
			})
		}
		count := strconv.Itoa(len(l.items))
		at(ui.Text(c, count), 220, 62, 50, 24)
		at(ui.Text(c, "stable"), 280, 62, 80, 24)
		at(ui.Text(c, count), 370, 62, 50, 24)
		at(ui.Box(c), 20, 110, 920, 160).Background(adapter.RGB(surface)).Clip().Children(func() {
			at(ui.Text(c, l.work.VisibleText()), 8, 8, 904, 144)
		})
		at(ui.Box(c), 20, 290, 920, w.ViewportHeight).Background(adapter.RGB(surface)).Clip().
			HandleInput(l.wheel).Children(func() {
			first, end := l.work.RowRange()
			for i := first; i < end; i++ {
				bg := uint32(surface)
				if i%2 != 0 {
					bg = stripe
				}
				at(ui.Box(c), 0, float32(i)*w.RowHeight-l.work.ScrollOffset, 920, w.RowHeight).
					Background(adapter.RGB(bg)).Children(func() {
					at(ui.Text(c, w.RowLabel(i)), 8, 4, 904, 20)
				})
			}
		})
	})
}

// append adds three items, reporting -1 if the fourth slot's limit stops
// it (the earlier writes stay).
func (l *lab) append() {
	l.done = 0
	for _, v := range []int{1, 2, 3} {
		if len(l.items) == 4 {
			l.done = -1
			return
		}
		l.items = append(l.items, v)
	}
}

// write makes two equal writes to the first item.
func (l *lab) write() {
	if len(l.items) == 0 {
		l.done = -1
		return
	}
	l.items[0] = 99
	l.items[0] = 99
	l.done = 0
}

func (l *lab) wheel(ev ui.InputEvent) bool {
	if ev.Kind != ui.InputScroll {
		return false
	}
	l.work.ScrollOffset = min(max(l.work.ScrollOffset+ev.DY, 0), float32(w.RowCount)*w.RowHeight-w.ViewportHeight)
	return true
}

// tick advances the workload: in stream the text changes, in scroll the
// list, so each tick has something new to draw.
func (l *lab) tick() bool {
	l.work.Tick()
	return true
}

func main() {
	l := &lab{work: w.FromEnv(), items: make([]int, 0, 4)}
	adapter.Window("MyGo performance lab", w.Width, w.Height, l.view, func(win *mygo.Window) {
		adapter.Ticks(win, l.work.Seconds, l.work.Mode != w.Idle, nil, l.tick, func() {
			fmt.Fprintf(os.Stderr, "workload_ticks=%d\n", l.work.Frames)
		})
	})
}
