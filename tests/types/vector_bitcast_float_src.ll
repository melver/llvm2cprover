; ---
; description = "a bitcast with non-integer vector lanes (<2 x float> to i64) fails closed"
; expected = "TRANSLATION FAILED"
; entry = "f"
; ---

define i64 @f(<2 x float> %v) {
entry:
  %r = bitcast <2 x float> %v to i64
  ret i64 %r
}
