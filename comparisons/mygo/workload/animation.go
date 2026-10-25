package workload

import "math"

// The animate scene: every dot's opacity and every shimmer character's
// colour change on every frame (the crate's `animation`).
const (
	AnimColumns    = 12
	AnimRows       = 4
	Dot            = float32(14)
	DotGap         = float32(12)
	Shimmer        = "Reading workspace files and streaming tokens"
	DotColor       = 0x9ad2ff
	AnimBackground = 0x10141c
)

var (
	AnimOrigin    = [2]float32{40, 120}
	ShimmerOrigin = [2]float32{40, 260}
)

// DotPosition is dot index's top left.
func DotPosition(index int) (float32, float32) {
	column, row := index%AnimColumns, index/AnimColumns
	return AnimOrigin[0] + float32(float32(column)*(Dot+DotGap)), AnimOrigin[1] + float32(float32(row)*(Dot+DotGap))
}

// DotOpacity is a 750 ms blink with a per-dot phase offset.
func DotOpacity(index int, seconds float32) float32 {
	phase := remEuclid(seconds/0.75-float32(float32(index)*0.07), 1)
	switch {
	case phase < 0.45:
		p := 1 - phase/0.45
		return 1 - float32(0.9*(1-float32(float32(p*p)*p)))
	case phase < 0.92:
		return 0.1
	}
	return 1
}

// ShimmerAlpha is white text's alpha under a band sweeping every 3.4 s.
func ShimmerAlpha(index, count int, seconds float32) uint8 {
	last := float32(max(count, 2) - 1)
	centre := -0.4 + float32(1.8*remEuclid(seconds/3.4, 1))
	x := 1 - float32(math.Abs(float64(float32(index)/last-centre)))/0.36
	x = min(max(x, 0), 1)
	return uint8(90 + float32(float32(float32(165*x)*x)*(3-float32(2*x))))
}
