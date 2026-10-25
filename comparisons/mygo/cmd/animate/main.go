// Animate is the display-rate animation scene (see the workload's
// animation.go) in MyGo's native UI: every dot's opacity and every shimmer
// character's colour change on every frame, which the view asks for with
// AnimationFrame, its clock at the frame's time (Context.Now).
//
// The shimmer is one ui.RichText with a span for each character, MyGo's way
// to colour characters apart: the zgx and GPUI adapters build a text for
// each character in a row, but in MyGo a text of a space alone has no width
// (as CSS drops a flex item of white space), so the words would run
// together.
//
//	ZGX_SECONDS=10 bin/animate
package main

import (
	"fmt"
	"os"
	"time"

	"github.com/egoist/mygo"
	"github.com/egoist/mygo/ui"
	"github.com/zeronsh/zgx/comparisons/mygo/internal/adapter"
	w "github.com/zeronsh/zgx/comparisons/mygo/workload"
)

type scene struct {
	start, last time.Time
	frames      uint64
	spans       []ui.Span
}

func (s *scene) view(c *ui.Context) {
	now := c.Now()
	// A frame can build its view more than once: count frames by their
	// time.
	if !now.Equal(s.last) {
		s.frames++
		s.last = now
	}
	c.AnimationFrame()
	t := float32(now.Sub(s.start).Seconds())
	ui.Box(c).Fill().Background(adapter.RGB(w.AnimBackground)).FontSize(18).Children(func() {
		for i := range w.AnimColumns * w.AnimRows {
			x, y := w.DotPosition(i)
			ui.Box(c).Absolute().Left(x).Top(y).Size(w.Dot, w.Dot).Radius(w.Dot / 2).
				Background(adapter.RGB(w.DotColor)).Opacity(w.DotOpacity(i, t))
		}
		for i := range s.spans {
			s.spans[i].Color = ui.Color{R: 255, G: 255, B: 255, A: w.ShimmerAlpha(i, len(s.spans), t)}
		}
		ui.RichText(c, s.spans...).Absolute().Left(w.ShimmerOrigin[0]).Top(w.ShimmerOrigin[1]).NoWrap()
	})
}

func main() {
	s := &scene{start: time.Now()}
	for _, r := range w.Shimmer {
		s.spans = append(s.spans, ui.Span{Text: string(r)})
	}
	adapter.Window("MyGo animation", w.Width, w.Height, s.view, func(win *mygo.Window) {
		adapter.Ticks(win, w.SecondsFromEnv(), false, nil, nil, func() {
			fmt.Fprintf(os.Stderr, "animation_frames=%d fps=%.1f\n", s.frames, float64(s.frames)/time.Since(s.start).Seconds())
		})
	})
}
