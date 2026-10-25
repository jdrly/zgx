// Package adapter is what the MyGo adapters share: their window, and the
// workload's clock, which every adapter of every framework runs the same
// way (comparisons/README.md).
package adapter

import (
	"log"
	"time"

	"github.com/egoist/mygo"
	"github.com/egoist/mygo/ui"
	"github.com/zeronsh/zgx/comparisons/mygo/workload"
)

// RGB is a 0xrrggbb colour.
func RGB(c uint32) ui.Color { return ui.RGB(uint8(c>>16), uint8(c>>8), uint8(c)) }

// Window shows view in a window of the scene's size (its content), not
// resizable, and runs the app until it quits.
func Window(title string, width, height float64, view func(*ui.Context), ready func(*mygo.Window)) {
	mygo.App.WhenReady(func() {
		win := mygo.NewWindow(mygo.WindowOptions{
			Title:          title,
			Width:          int(width),
			Height:         int(height),
			UseContentSize: true,
			DisableResize:  true,
			Content:        ui.View(view),
		})
		ready(win)
	})
	if err := mygo.App.Run(); err != nil {
		log.Fatal(err)
	}
}

// Ticks runs tick on the window's thread once a workload.Period by the wall
// clock, dropping missed deadlines, while active, and redraws when tick
// reports a change: MyGo draws only when asked, and a tick that moved
// nothing draws nothing. When seconds pass (never for 0) it runs report
// there and quits. A tick whose turn returns false (the heavy scene's
// ZGX_PARITY) is skipped.
func Ticks(win *mygo.Window, seconds float64, active bool, turn func() bool, tick func() bool, report func()) {
	start := time.Now()
	end := start.Add(time.Duration(seconds * float64(time.Second)))
	finish := func() {
		mygo.RunOnMain(report)
		mygo.App.Quit()
	}
	run := func() {
		if tick() {
			win.Invalidate()
		}
	}
	go func() {
		if !active {
			if seconds > 0 {
				time.Sleep(time.Until(end))
				finish()
			}
			return
		}
		next := start.Add(workload.Period)
		for {
			wake := next
			if seconds > 0 && end.Before(wake) {
				wake = end
			}
			time.Sleep(time.Until(wake))
			if seconds > 0 && !time.Now().Before(end) {
				finish()
				return
			}
			if turn == nil || turn() {
				// RunOnMain waits, as the Rust adapters' loops wait for
				// their tick, so a busy main thread drops ticks rather than
				// queueing them.
				mygo.RunOnMain(run)
			}
			next = next.Add(workload.Period)
			if now := time.Now(); !next.After(now) {
				next = now.Add(workload.Period)
			}
		}
	}()
}
