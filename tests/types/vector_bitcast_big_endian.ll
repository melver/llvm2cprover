; ---
; description = "non-identity vector bitcasts fail closed on big-endian targets"
; expected = "TRANSLATION FAILED"
; entry = "f"
; ---
;
; Lane packing assumes lane 0 in the least-significant bits (little endian).

target datalayout = "E-m:e-i64:64-n32:64-S128"

define <32 x i1> @f(<2 x i16> %v) {
entry:
  %r = bitcast <2 x i16> %v to <32 x i1>
  ret <32 x i1> %r
}
