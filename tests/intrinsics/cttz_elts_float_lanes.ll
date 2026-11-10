; ---
; description = "llvm.experimental.cttz.elts over non-integer lanes fails closed"
; expected = "TRANSLATION FAILED"
; entry = "main"
; ---
;
; LangRef defines the intrinsic for integer lanes only; the IR verifier accepts floats.

declare i32 @llvm.experimental.cttz.elts.i32.v4f32(<4 x float>, i1)

define i32 @main() {
entry:
  %n = call i32 @llvm.experimental.cttz.elts.i32.v4f32(<4 x float> zeroinitializer, i1 false)
  ret i32 %n
}
