; ---
; description = "llvm.experimental.cttz.elts whose return type cannot hold the lane count fails closed"
; expected = "TRANSLATION FAILED"
; entry = "main"
; ---
;
; LangRef: undefined behavior for every input, and the lane count itself would not fit.

declare i2 @llvm.experimental.cttz.elts.i2.v4i1(<4 x i1>, i1)

define i32 @main() {
entry:
  %n = call i2 @llvm.experimental.cttz.elts.i2.v4i1(<4 x i1> zeroinitializer, i1 false)
  ret i32 0
}
