package workload

import (
	"fmt"
	"strings"
)

// The variable-height list scene: 100,000 deterministic chat rows of
// wrapped text, 1 to 8 lines each, about one in four with a fixed-size
// image placeholder under its text (the crate's `vlist`). Modes: a steady
// scroll, flings (a velocity curve), and a steady scroll with a jump to a
// seeded row every JumpEvery ticks.
const (
	VWidth      = float32(720)
	VHeight     = float32(800)
	VRows       = 100_000
	VFontSize   = float32(14)
	VLineHeight = float32(20)
	// VGap is the gap between a row's parts.
	VGap = float32(6)
	// VTextWidth is the text's width: the window less the padding.
	VTextWidth  = VWidth - 2*16
	VBackground = 0x0e1218
	VAuthor     = 0x5eb1ff
	VText       = 0xe5edf7
	VMuted      = 0x8b98ab
	VImageColor = 0x2b3a52
	// ScrollStep is the pixels a scroll tick adds.
	ScrollStep = float32(12)
	// JumpEvery is the ticks between jumps.
	JumpEvery = 30
)

var (
	// VPad is a row's padding around its content.
	VPad      = [2]float32{16, 10}
	VImage    = [2]float32{160, 90}
	RowColors = [2]uint32{0x161c26, 0x19202b}
	// Fling is the first tick's velocity, its decay a tick, and the ticks
	// before the next fling.
	Fling = struct {
		Velocity, Decay float32
		Every           uint64
	}{160, 0.94, 90}
)

const wordsPerLine = 16

var words = [48]string{
	"the", "build", "passed", "on", "main", "after", "we", "moved", "layout", "into", "its", "own",
	"pass", "and", "a", "list", "of", "rows", "now", "measures", "each", "height", "once", "text",
	"wraps", "at", "the", "width", "so", "scrolling", "stays", "smooth", "even", "with", "images", "and",
	"long", "messages", "from", "everyone", "who", "reviewed", "it", "today", "thanks", "for", "looking", "again",
}

// VMode is how the list moves: a steady scroll, flings, or jumps to rows.
type VMode int

const (
	VScroll VMode = iota
	VFling
	VJump
)

// VModeFromEnv reads ZGX_MODE: scroll (the default), fling or jump.
func VModeFromEnv() VMode {
	switch Env("ZGX_MODE") {
	case "fling":
		return VFling
	case "jump":
		return VJump
	}
	return VScroll
}

// Hash is splitmix64: a row's (or a jump's) deterministic randomness.
func Hash(x uint64) uint64 {
	z := x + 0x9e37_79b9_7f4a_7c15
	z = (z ^ (z >> 30)) * 0xbf58_476d_1ce4_e5b9
	z = (z ^ (z >> 27)) * 0x94d0_49bb_1331_11eb
	return z ^ (z >> 31)
}

// Lines is the lines row's text is written to fill, 1 to 8.
func Lines(row int) int { return 1 + int(Hash(uint64(row))%8) }

func HasImage(row int) bool { return (Hash(uint64(row))>>8)%4 == 0 }

func Author(row int) string { return fmt.Sprintf("user-%04d", Hash(uint64(row)^0xa5a5)%10_000) }

func Time(row int) string {
	minutes := (row / 3) % (24 * 60)
	return fmt.Sprintf("%02d:%02d", minutes/60, minutes%60)
}

// Text is row's message: sixteen words a line.
func Text(row int) string {
	n := Lines(row) * wordsPerLine
	seed := Hash(uint64(row) + 0x5151)
	var b strings.Builder
	b.Grow(n * 7)
	for i := range n {
		seed = Hash(seed)
		if i > 0 {
			b.WriteByte(' ')
		}
		b.WriteString(words[seed%uint64(len(words))])
	}
	return b.String()
}

// Delta is the scroll a tick adds (every mode; a jump's tick lands on its
// row instead).
func Delta(mode VMode, tick uint64) float32 {
	if mode == VFling {
		return Fling.Velocity * powi(Fling.Decay, int32(tick%Fling.Every))
	}
	// Jump scrolls between its jumps too.
	return ScrollStep
}

// Jump is the row a jump lands on at tick, if one does then.
func Jump(tick uint64) (int, bool) {
	if tick > 0 && tick%JumpEvery == 0 {
		return int(Hash(tick^0x1234_5678) % VRows), true
	}
	return 0, false
}
