; ---
; description = "a bitcast with non-integer vector lanes (i64 to <2 x float>) fails closed"
; expected = "TRANSLATION FAILED"
; entry = "f"
; ---

define <2 x float> @f(i64 %v) {
entry:
  %r = bitcast i64 %v to <2 x float>
  ret <2 x float> %r
}
