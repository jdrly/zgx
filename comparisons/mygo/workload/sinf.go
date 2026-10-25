package workload

import "math"

// Sinf is macOS's libm sinf (libsystem_m, macOS 26 on arm64), the one
// Rust's f32::sin calls there, in Go: the same reduction, polynomials and
// roundings, fused where it fuses, so it returns the same float32 for every
// input (libm_test.go checks). Apple's sinf is within an ulp of the sine
// but not correctly rounded: a float32 of math.Sin differs from it for
// 1.8% of inputs, enough to change a value the scenes show. Calling libm
// instead (cgo or purego) costs some 140 ns a call against 3 here, and the
// heavy dashboard takes about 2,900 sines a tick.
func Sinf(x float32) float32 {
	ix := math.Float32bits(x) & 0x7fffffff
	switch {
	case ix >= 0x7f800000: // inf, NaN
		return x - x
	case ix >= 0x42f00000: // |x| >= 120: reduce by π with a table of 1/π's bits
		return sinfLarge(x, ix)
	case ix >= 0x3f490fdb: // |x| >= π/4: reduce by π/2
		d := float64(x)
		k := float64(d * 0x1.45f306dc9c883p-1)
		q := uint64(roundToInt(k))
		k = math.RoundToEven(k)
		r := math.FMA(-k, 0x1.921fb54442d18p+0, d)
		sign := (q >> 1) << 63
		var y float64
		if q&1 != 0 {
			y = cosPoly(r)
		} else {
			y = sinPoly(r)
		}
		return float32(math.Float64frombits(math.Float64bits(y) ^ sign))
	case ix >= 0x39800000: // |x| >= 2⁻¹²
		return float32(sinPoly(float64(x)))
	}
	// Tiny: x, rounded as fma(x, 2²⁶, x)·2⁻²⁶ rounds it (exact in float64).
	return float32(float64(x)*(0x1p26+1)) * 0x1p-26
}

func sinPoly(r float64) float64 {
	z := float64(r * r)
	rz := float64(r * z)
	a := math.FMA(z, -0x1.9943e0fc4818fp-13, 0x1.11073afd145ebp-7)
	z3 := float64(z * rz)
	b := math.FMA(rz, -0x1.555545268a02ap-3, r)
	return math.FMA(z3, a, b)
}

func cosPoly(r float64) float64 {
	z := float64(r * r)
	a := math.FMA(z, 0x1.99169fc938f4ap-16, -0x1.6c07f16945df4p-10)
	b := math.FMA(z, 0x1.55553c7899699p-5, -0x1.ffffffcb82e97p-2)
	z2 := float64(z * z)
	a = math.FMA(z2, a, b)
	return math.FMA(z, a, 1)
}

func sinfLarge(x float32, ix uint32) float32 {
	d := float64(x)
	t := &sinfTable[(ix>>24)-0x40]
	hi := float64(d * math.Float64frombits(t[0]))
	lo := float64(d * math.Float64frombits(t[1]))
	q := uint64(roundToInt(hi))
	f := float64(hi-math.RoundToEven(hi)) + lo
	s := math.Float64frombits(math.Float64bits(f) ^ (q << 63))
	z := float64(f * f)
	a := math.FMA(z, float64(z+-0x1.0a8b9fc97b22cp+2), 0x1.ef88f0b1250c1p+3)
	b := math.FMA(z, float64(z+-0x1.d31f4a0a5523p+1), 0x1.537b6744fa779p+1)
	return float32(float64(s*a) * float64(0x1.39502de83d49cp-4*b))
}

// roundToInt is arm64's fcvtns: to the nearest integer, ties to even,
// saturating.
func roundToInt(v float64) int64 {
	r := math.RoundToEven(v)
	switch {
	case r != r:
		return 0
	case r >= 0x1p63:
		return math.MaxInt64
	case r < -0x1p63:
		return math.MinInt64
	}
	return int64(r)
}

// sinfTable is, by a float's exponent byte less 0x40, the high and the low
// part of the bits of 1/π that matter at that exponent.
var sinfTable = [64][2]uint64{
	{0x3fd45f306e000000, 0xbdeb1bbead603d8b},
	{0x3fd45f306e000000, 0xbdeb1bbead603d8b},
	{0x3fd45f306e000000, 0xbdeb1bbead603d8b},
	{0x3fd45f306e000000, 0xbdeb1bbead603d8b},
	{0x3fd45f306e000000, 0xbdeb1bbead603d8b},
	{0x3fd45f306e000000, 0xbdeb1bbead603d8b},
	{0x3fd45f306e000000, 0xbdeb1bbead603d8b},
	{0x3fd45f306e000000, 0xbdeb1bbead603d8b},
	{0x3fd45f306e000000, 0xbdeb1bbead603d8b},
	{0x3fd45f306e000000, 0xbdeb1bbead603d8b},
	{0x3fd45f306e000000, 0xbdeb1bbead603d8b},
	{0x3fd45f306e000000, 0xbdeb1bbead603d8b},
	{0xbfc7419f24000000, 0xbdeb1bbead603d8b},
	{0xbfad067c92000000, 0x3dc391054a7f09d6},
	{0x3f77cc1b72000000, 0x3d9c882a53f84eb0},
	{0xbf6067c91b000000, 0xbd6bbead603d8a83},
	{0xbf09f246c7000000, 0x3d0054a7f09d5f48},
	{0xbf09f246c7000000, 0x3d0054a7f09d5f48},
	{0xbf09f246c7000000, 0x3d0054a7f09d5f48},
	{0x3ee836e4e4000000, 0x3d0054a7f09d5f48},
	{0xbecf246c6f000000, 0x3ca529fc2757d1f5},
	{0x3e7b727221000000, 0xbc95ac07b1505c16},
	{0x3e7b727221000000, 0xbc95ac07b1505c16},
	{0xbe5236377d000000, 0xbc76b01ec5417056},
	{0x3e4b939105000000, 0x3c629fc2757d1f53},
	{0xbe21b1bbeb000000, 0x3c34fe13abe8fa9a},
	{0xbdeb1bbead000000, 0xbc080f62a0b82b2d},
	{0xbdeb1bbead000000, 0xbc080f62a0b82b2d},
	{0x3dc391054a000000, 0x3befc2757d1f534e},
	{0x3d9c882a54000000, 0xbb7ec54170565912},
	{0xbd6bbead60000000, 0xbb7ec54170565912},
	{0xbd6bbead60000000, 0xbb7ec54170565912},
	{0x3d41054a7f000000, 0x3b33abe8fa9a6ee0},
	{0x3d0054a7f1000000, 0xbb28a82e0acb223f},
	{0x3d0054a7f1000000, 0xbb28a82e0acb223f},
	{0x3ca529fc27000000, 0x3ac5f47d4d377037},
	{0x3ca529fc27000000, 0x3ac5f47d4d377037},
	{0x3ca529fc27000000, 0x3ac5f47d4d377037},
	{0x3c84a7f09d000000, 0x3aa7d1f534ddc0db},
	{0x3c629fc275000000, 0x3a8f47d4d377036e},
	{0x3c34fe13ac000000, 0xba370565911f924f},
	{0xbc2603d8a8000000, 0xba370565911f924f},
	{0xbc080f62a1000000, 0x3a21f534ddc0db63},
	{0x3befc2757d000000, 0x39ef534ddc0db629},
	{0xbb7ec54170000000, 0xb99596447e493ad5},
	{0xbb7ec54170000000, 0xb99596447e493ad5},
	{0xbb7ec54170000000, 0xb99596447e493ad5},
	{0x3b33abe8fb000000, 0xb9596447e493ad4d},
	{0x3b33abe8fb000000, 0xb9596447e493ad4d},
	{0xbb28a82e0b000000, 0x393a6ee06db14acd},
	{0x3b0d5f47d5000000, 0xb916447e493ad4ce},
	{0xbad505c159000000, 0xb8f911f924eb5336},
	{0x3ac5f47d4d000000, 0x38dbb81b6c52b328},
	{0x3aa7d1f535000000, 0xb8b11f924eb53362},
	{0x3a8f47d4d3000000, 0x38adc0db6295993c},
	{0xba37056591000000, 0xb83f924eb53361de},
	{0xba37056591000000, 0xb83f924eb53361de},
	{0x3a21f534de000000, 0xb83f924eb53361de},
	{0x39ef534ddc000000, 0x37db6c52b3278872},
	{0x39ef534ddc000000, 0x37db6c52b3278872},
	{0xb99596447e000000, 0xb7b24eb53361de38},
	{0xb99596447e000000, 0xb7b24eb53361de38},
	{0x3984d37703000000, 0x37ab6295993c4390},
	{0x39634ddc0e000000, 0xb78275a99b0ef1bf},
}
