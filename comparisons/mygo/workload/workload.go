// Package workload is comparisons/workload (the zgx-workload crate) in Go:
// the same deterministic text, numbers and geometry, so the MyGo adapters
// show what the zgx and GPUI adapters show. testdata/golden.txt is the
// crate's output (its `golden` example); the test checks this package
// prints it byte for byte.
//
// Float arithmetic is float32 as the crate's is, each product converted to
// float32 on its own: the conversion stops Go fusing a multiply into the
// add that follows (it does on arm64), which Rust never does.
package workload

import (
	"fmt"
	"math"
	"os"
	"strconv"
	"strings"
	"time"
)

const (
	Width          = 960.0
	Height         = 720.0
	RowHeight      = float32(28.0)
	RowCount       = 100_000
	ViewportHeight = float32(400.0)
	Period         = 16_666_667 * time.Nanosecond
	Token          = "The quick brown fox streams a token. "
)

// Font is every scene's font: the one file in comparisons/fonts, which each
// adapter loads itself (FontFile), so every framework shapes the same
// glyphs.
const Font = "DejaVu Sans"

// FNV64 is FNV-1a, 64-bit: how an adapter names the font file it loaded.
func FNV64(b []byte) uint64 {
	h := uint64(0xcbf2_9ce4_8422_2325)
	for _, c := range b {
		h = (h ^ uint64(c)) * 0x100_0000_01b3
	}
	return h
}

// FontFile is the font file the runner staged (ZGX_FONT_FILE), read and
// reported as the runner checks it (font_family=… font_file_fnv64=…, on
// stderr), or nil when there is none.
func FontFile() []byte {
	path := Env("ZGX_FONT_FILE")
	if path == "" {
		return nil
	}
	b, err := os.ReadFile(path)
	if err != nil {
		return nil
	}
	fmt.Fprintf(os.Stderr, "font_family=%s font_file_fnv64=%016x\n", Font, FNV64(b))
	return b
}

// Mode is what a run advances: nothing, the text, the scroll, or both.
type Mode int

const (
	Idle Mode = iota
	Stream
	Scroll
	Both
)

// Env is os.Getenv(key), or for one release the name it had before the
// rename to zgx.
func Env(key string) string {
	if v, ok := os.LookupEnv(key); ok {
		return v
	}
	return os.Getenv(strings.Replace(key, "ZGX_", "ZGUI_", 1)) // rename-to-zgx: keep
}

// ModeFromEnv reads ZGX_MODE (both by default).
func ModeFromEnv() Mode {
	switch Env("ZGX_MODE") {
	case "idle":
		return Idle
	case "stream":
		return Stream
	case "scroll":
		return Scroll
	}
	return Both
}

// SecondsFromEnv reads ZGX_SECONDS (30 by default).
func SecondsFromEnv() float64 {
	if v, err := strconv.ParseFloat(Env("ZGX_SECONDS"), 64); err == nil && v >= 0 && !math.IsInf(v, 0) {
		return v
	}
	return 30
}

// remEuclid is Rust's f32::rem_euclid.
func remEuclid(a, b float32) float32 {
	r := float32(math.Mod(float64(a), float64(b)))
	if r < 0 {
		r += float32(math.Abs(float64(b)))
	}
	return r
}

// powi is Rust's f32::powi with an exponent known only at run time:
// compiler-rt's __powisf2, squaring as it goes.
func powi(a float32, b int32) float32 {
	recip := b < 0
	n := uint32(b)
	if recip {
		n = uint32(-int64(b))
	}
	r := float32(1)
	for {
		if n&1 != 0 {
			r *= a
		}
		n >>= 1
		if n == 0 {
			break
		}
		a *= a
	}
	if recip {
		return 1 / r
	}
	return r
}

// Workload is the lab scene's streaming text and scroll offset.
type Workload struct {
	text         []byte
	ScrollOffset float32
	Frames       uint64
	Mode         Mode
	Seconds      float64
}

// New is a workload in mode with no ticks run.
func New(mode Mode, seconds float64) *Workload {
	return &Workload{text: make([]byte, 0, 8256), Mode: mode, Seconds: seconds}
}

// FromEnv is a workload as ZGX_MODE and ZGX_SECONDS ask (ZGX_INITIAL_TICKS
// ticks of both first).
func FromEnv() *Workload {
	w := New(ModeFromEnv(), SecondsFromEnv())
	initial, _ := strconv.ParseUint(strings.TrimPrefix(Env("ZGX_INITIAL_TICKS"), "+"), 10, 64)
	initial = min(initial, 100_000)
	mode := w.Mode
	w.Mode = Both
	for range initial {
		w.Tick()
	}
	w.Mode = mode
	return w
}

// Tick appends the next token (stream, both) and scrolls 14 px (scroll,
// both), keeping the last 8192 bytes of the text.
func (w *Workload) Tick() {
	w.Frames++
	if w.Mode == Stream || w.Mode == Both {
		w.text = fmt.Appendf(w.text, "%06d %s", w.Frames, Token)
		if n := len(w.text); n > 8192 {
			w.text = w.text[:copy(w.text, w.text[n-8192:])]
		}
	}
	if w.Mode == Scroll || w.Mode == Both {
		w.ScrollOffset = float32(math.Mod(float64(w.ScrollOffset+14), float64(float32(RowCount)*RowHeight-ViewportHeight)))
	}
}

// Text is the streamed text kept.
func (w *Workload) Text() string { return string(w.text) }

// RowRange is the rows to build, [first, end): those in view and two more
// each side.
func (w *Workload) RowRange() (int, int) {
	first := int(w.ScrollOffset / RowHeight)
	end := int(math.Ceil((float64(w.ScrollOffset) + float64(ViewportHeight)) / float64(RowHeight)))
	return max(first-2, 0), min(end+2, RowCount)
}

// VisibleText is the text's last 735 bytes in lines of 105.
func (w *Workload) VisibleText() string {
	tail := w.text[max(len(w.text)-735, 0):]
	var b strings.Builder
	b.Grow(len(tail) + 6)
	for i := 0; len(tail) > 0 && i < 7; i++ {
		if i > 0 {
			b.WriteByte('\n')
		}
		n := min(105, len(tail))
		b.Write(tail[:n])
		tail = tail[n:]
	}
	return b.String()
}

// RowLabel is the lab list's row text.
func RowLabel(index int) string { return fmt.Sprintf("Row %06d — retained virtual item", index) }
