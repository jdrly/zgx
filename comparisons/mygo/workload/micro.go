package workload

import (
	"fmt"
	"strconv"
)

// The microbenchmarks (the crate's `micro`).
//
// state: a static grid of StaticRows × StaticColumns texts (StaticLabel),
// each Cell points, under one text bound to a counter (CounterLabel), in a
// Width × Height window.
//
// vscroll: MicroRows rows in a VWidth × MicroViewport window, fixed
// (FixedRow points of FixedLabel) or variable (the vlist rows, measured as
// they mount); a step scrolls MicroStep points, back to the top past the
// end. Operations are timed after MicroWarmup untimed ones, in batches.
const (
	StaticRows    = 40
	StaticColumns = 25
	MicroFontSize = float32(12)
	MicroRows     = 10_000
	MicroViewport = float32(800)
	FixedRow      = float32(28)
	MicroStep     = float32(12)
	MicroOps      = 2_000
	MicroWarmup   = 200
)

var Cell = [2]float32{36, 18}

func StaticLabel(index int) string { return strconv.Itoa(index) }

func CounterLabel(value uint64) string { return fmt.Sprintf("Count: %d", value) }

func FixedLabel(index int) string { return fmt.Sprintf("Row %d: %s", index, Author(index)) }
