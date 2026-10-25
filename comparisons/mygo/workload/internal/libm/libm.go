//go:build cgo

// Package libm calls the platform's libm, for tests that check the
// workload's Go ports of its functions against it. The adapters never
// import it: they build without cgo.
package libm

/*
#include <math.h>
*/
import "C"

// Sinf is the platform's sinf.
func Sinf(x float32) float32 { return float32(C.sinf(C.float(x))) }
