; ---
; description = "vector llvm.bitreverse fails closed instead of crashing"
; expected = "TRANSLATION FAILED"
; entry = "main"
; ---

declare <2 x i8> @llvm.bitreverse.v2i8(<2 x i8>)

define i32 @main() {
entry:
  %r = call <2 x i8> @llvm.bitreverse.v2i8(<2 x i8> <i8 1, i8 2>)
  %e = extractelement <2 x i8> %r, i64 0
  %z = zext i8 %e to i32
  ret i32 %z
}
