// Heavy is the heavy dashboard (see the workload's heavy.go) in MyGo's
// native UI, as the zgx and GPUI Fast adapters draw it: tiles, service cards
// with a value, a progress bar and a sparkline each, a table showing the
// rows in view (and two each side) and a streaming log. MyGo builds the
// view anew for each frame after a tick, as its views are written.
//
//	ZGX_MODE=both ZGX_SECONDS=10 bin/heavy
package main

import (
	"fmt"
	"os"

	"github.com/egoist/mygo"
	"github.com/egoist/mygo/ui"
	"github.com/zeronsh/zgx/comparisons/mygo/internal/adapter"
	w "github.com/zeronsh/zgx/comparisons/mygo/workload"
)

// at places e at (x, y) in its parent, w×h.
func at(e ui.Element, x, y, width, height float32) ui.Element {
	return e.Absolute().Left(x).Top(y).Size(width, height)
}

func panel(e ui.Element, radius float32) ui.Element {
	return e.Radius(radius).Border(1, adapter.RGB(w.Border)).Background(adapter.RGB(w.Panel))
}

func label(c *ui.Context, s string, size, line float32) ui.Element {
	return ui.Text(c, s).FontSize(size).FixedLineHeight(line)
}

type dashboard struct{ work *w.Heavy }

func (d *dashboard) view(c *ui.Context) {
	live := d.work.Live
	muted := adapter.RGB(w.Muted)
	ui.Box(c).Fill().Background(adapter.RGB(w.Background)).Font(w.Font).TextColor(adapter.RGB(w.TextColor)).Children(func() {
		at(label(c, w.Title, 18, 24), w.TitleOrigin[0], w.TitleOrigin[1], 600, 24)
		for i := range w.Tiles {
			x, y := w.TilePosition(i)
			panel(at(ui.Box(c), x, y, w.Tile[0], w.Tile[1]), 8).Children(func() {
				at(label(c, w.TileLabel(i), 11, 14).TextColor(muted), w.TileLabelAt[0], w.TileLabelAt[1], 176, 14)
				at(label(c, w.TileValue(i, live), 20, 26), w.TileValueAt[0], w.TileValueAt[1], 176, 26)
			})
		}
		for i := range w.Cards {
			x, y := w.CardPosition(i)
			panel(at(ui.Box(c), x, y, w.Card[0], w.Card[1]), 6).Children(func() {
				at(label(c, w.CardTitle(i), 11, 14).TextColor(muted), w.CardTitleAt[0], w.CardTitleAt[1], 104, 14)
				at(label(c, w.CardValue(i, live), 16, 20), w.CardValueAt[0], w.CardValueAt[1], 104, 20)
				p := w.Progress
				at(ui.Box(c), p[0], p[1], p[2], p[3]).Radius(2).Background(adapter.RGB(w.Track))
				at(ui.Box(c), p[0], p[1], w.CardProgress(i, live), p[3]).Radius(2).Background(adapter.RGB(w.Accent))
				for bar := range w.Bars {
					h := w.BarHeight(i, bar, live)
					at(ui.Box(c), w.BarXAt(bar), w.SparkBase-h, w.BarWidth, h).Background(adapter.RGB(w.Accent))
				}
			})
		}
		d.table(c)
		panel(at(ui.Box(c), w.LogOrigin[0], w.LogOrigin[1], w.Log[0], w.Log[1]), 8).Children(func() {
			at(label(c, d.work.VisibleLog(), 11, w.LogLineHeight).TextColor(muted),
				w.LogTextAt[0], w.LogTextAt[1], w.Log[0]-24, w.LogLines*w.LogLineHeight)
		})
	})
}

func (d *dashboard) table(c *ui.Context) {
	live, scroll := d.work.Live, d.work.Scroll
	muted := adapter.RGB(w.Muted)
	panel(at(ui.Box(c), w.TableOrigin[0], w.TableOrigin[1], w.Table[0], w.Table[1]), 8).Clip().Children(func() {
		at(ui.Box(c), 0, 0, w.Table[0], w.TableHeader).Background(adapter.RGB(w.HeaderColor)).Children(func() {
			for _, col := range w.Columns {
				at(label(c, col.Title, 11, 14).TextColor(muted), col.X, 7, 80, 14)
			}
		})
		at(ui.Box(c), 0, w.TableHeader, w.Table[0], w.TableViewport).Clip().Children(func() {
			first, end := d.work.RowRange()
			for row := first; row < end; row++ {
				status, color := w.Status(row, live)
				cell := func(s string, column int, width float32) ui.Element {
					return at(label(c, s, 12, 16), w.Columns[column].X, w.CellTop, width, 16)
				}
				at(ui.Box(c), 0, float32(row)*w.TableRow-scroll, w.Table[0], w.TableRow).
					Background(adapter.RGB(w.RowColor(row))).Children(func() {
					cell(w.RowID(row), 0, 70).TextColor(muted)
					cell(w.RowWorker(row), 1, 150)
					at(ui.Box(c), w.Pill[0], w.Pill[1], w.Pill[2], w.Pill[3]).Radius(8).Background(adapter.RGB(color)).Children(func() {
						at(label(c, status, 10, 14), w.PillText[0], w.PillText[1], 56, 14)
					})
					cell(w.RowLatency(row, live), 3, 70)
					cell(w.RowThroughput(row, live), 4, 80)
				})
			}
		})
	})
}

// tick advances the dashboard: in stream the cards, tiles and log change,
// in scroll the table, so each tick has something new to draw.
func (d *dashboard) tick() bool {
	d.work.Tick()
	return true
}

func main() {
	d := &dashboard{work: w.HeavyFromEnv()}
	adapter.Window("MyGo heavy", w.HeavyWidth, w.HeavyHeight, d.view, func(win *mygo.Window) {
		adapter.Ticks(win, d.work.Seconds, d.work.Mode != w.Idle, w.Turn, d.tick, func() {
			fmt.Fprintf(os.Stderr, "workload_ticks=%d\n", d.work.Frames)
		})
	})
}
