; ---
; description = "vector ctpop/cttz/bswap fail closed instead of crashing"
; expected = "TRANSLATION FAILED"
; entry = "main"
; ---

declare <2 x i16> @llvm.bswap.v2i16(<2 x i16>)

define i32 @main() {
entry:
  %r = call <2 x i16> @llvm.bswap.v2i16(<2 x i16> <i16 1, i16 2>)
  %e = extractelement <2 x i16> %r, i64 0
  %z = zext i16 %e to i32
  ret i32 %z
}
