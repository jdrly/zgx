package workload

import (
	"bytes"
	"fmt"
	"os"
	"strconv"
	"strings"
	"testing"
)

// golden prints what the crate's `golden` example prints, line for line.
func golden() []byte {
	var out bytes.Buffer
	p := func(format string, args ...any) { fmt.Fprintf(&out, format+"\n", args...) }

	lab := New(Both, 0)
	for tick := range 1500 {
		lab.Tick()
		if tick%37 == 0 {
			first, end := lab.RowRange()
			p("lab %d %d..%d %s", tick, first, end, debug(lab.VisibleText()))
		}
	}
	for _, row := range []int{0, 1, 99, 12_345, 99_999} {
		p("label %d %s", row, RowLabel(row))
	}

	work := &Heavy{Mode: Both}
	for range LogLines {
		work.Tick()
	}
	for tick := range 900 {
		work.Tick()
		live := work.Live
		if tick%29 != 0 {
			continue
		}
		first, end := work.RowRange()
		p("heavy %d live %d scroll %s rows %d..%d", tick, live, f32(work.Scroll), first, end)
		p("log %s", debug(work.VisibleLog()))
		for tile := range Tiles {
			p("tile %d %s %s", tile, TileLabel(tile), TileValue(tile, live))
		}
		for _, card := range []int{0, 7, 19, 29} {
			bars := make([]string, Bars)
			for b := range Bars {
				bars[b] = fmt.Sprintf("%.3f", BarHeight(card, b, live))
			}
			p("card %d %s %s %.3f %s %s", card, CardTitle(card), CardValue(card, live),
				CardProgress(card, live), status(card, live), strings.Join(bars, ","))
		}
		for row := first; row < end; row += 7 {
			p("row %d %s %s %s %s %s %06x", row, RowID(row), RowWorker(row), RowLatency(row, live),
				RowThroughput(row, live), status(row, live), RowColor(row))
		}
	}

	count := len([]rune(Shimmer))
	for frame := range 240 {
		seconds := float32(frame) / 120
		var dots, shimmer []string
		for i := 0; i < AnimColumns*AnimRows; i += 5 {
			dots = append(dots, fmt.Sprintf("%.4f", DotOpacity(i, seconds)))
		}
		for i := 0; i < count; i += 3 {
			shimmer = append(shimmer, strconv.Itoa(int(ShimmerAlpha(i, count, seconds))))
		}
		p("animate %d %s %s", frame, strings.Join(dots, ","), strings.Join(shimmer, ","))
	}

	rows := make([]int, 0, 43)
	for row := range 40 {
		rows = append(rows, row)
	}
	for _, row := range append(rows, 999, 54_321, 99_999) {
		p("vlist %d %d %t %s %s %s", row, Lines(row), HasImage(row), Author(row), Time(row), debug(Text(row)))
	}
	for tick := range uint64(200) {
		jump := "None"
		if row, ok := Jump(tick); ok {
			jump = fmt.Sprintf("Some(%d)", row)
		}
		p("vtick %d %s %s %s %s", tick, f32(Delta(VScroll, tick)), f32(Delta(VFling, tick)), f32(Delta(VJump, tick)), jump)
	}
	return out.Bytes()
}

// debug is Rust's {:?} of a string.
func debug(s string) string {
	var b strings.Builder
	b.WriteByte('"')
	for _, r := range s {
		switch r {
		case '"':
			b.WriteString(`\"`)
		case '\\':
			b.WriteString(`\\`)
		case '\n':
			b.WriteString(`\n`)
		case '\r':
			b.WriteString(`\r`)
		case '\t':
			b.WriteString(`\t`)
		default:
			if r < 0x20 || r == 0x7f {
				fmt.Fprintf(&b, `\u{%x}`, r)
			} else {
				b.WriteRune(r)
			}
		}
	}
	b.WriteByte('"')
	return b.String()
}

// f32 is Rust's {} of an f32: the shortest decimal that reads back.
func f32(x float32) string { return strconv.FormatFloat(float64(x), 'f', -1, 32) }

// status is Rust's {:?} of a status: ("healthy", 1928010).
func status(series int, live uint64) string {
	label, color := Status(series, live)
	return fmt.Sprintf("(%s, %d)", debug(label), color)
}

func TestGolden(t *testing.T) {
	want, err := os.ReadFile("testdata/golden.txt")
	if err != nil {
		t.Fatal(err)
	}
	got := golden()
	if bytes.Equal(got, want) {
		return
	}
	gl, wl := strings.Split(string(got), "\n"), strings.Split(string(want), "\n")
	for i := range min(len(gl), len(wl)) {
		if gl[i] != wl[i] {
			t.Fatalf("line %d differs:\n got %s\nwant %s", i+1, gl[i], wl[i])
		}
	}
	t.Fatalf("%d lines, want %d", len(gl), len(wl))
}
