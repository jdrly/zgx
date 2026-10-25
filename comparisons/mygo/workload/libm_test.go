//go:build cgo && darwin && arm64

package workload

import (
	"flag"
	"math"
	"runtime"
	"sync"
	"testing"

	"github.com/zeronsh/zgx/comparisons/mygo/workload/internal/libm"
)

var allFloats = flag.Bool("libm.all", false, "check Sinf against libm for every float32 (about 30 s), not every 61st")

// Sinf is libm's sinf on this Mac: checked over every 61st bit pattern of
// float32 (every one with -libm.all), both signs, infinities and NaNs.
func TestSinfIsLibm(t *testing.T) {
	step := uint64(61)
	if *allFloats {
		step = 1
	}
	workers := uint64(max(runtime.NumCPU()/2, 1))
	var wg sync.WaitGroup
	var mu sync.Mutex
	failed := 0
	for w := range workers {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for b := w * step; b <= math.MaxUint32; b += workers * step {
				x := math.Float32frombits(uint32(b))
				got, want := Sinf(x), libm.Sinf(x)
				if math.Float32bits(got) != math.Float32bits(want) && !(got != got && want != want) {
					mu.Lock()
					if failed++; failed <= 5 {
						t.Errorf("Sinf(%g) [%#08x] = %g, libm %g", x, b, got, want)
					}
					mu.Unlock()
				}
			}
		}()
	}
	wg.Wait()
	if failed > 0 {
		t.Fatalf("%d inputs differ", failed)
	}
}
