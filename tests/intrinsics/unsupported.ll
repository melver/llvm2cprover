; ---
; description = "unsupported llvm intrinsics fail translation deterministically"
; expected = "TRANSLATION FAILED"
; entry = "main"
; ---

source_filename = "/tmp/unsupported_intrinsic.ll"

declare i32 @llvm.fshl.i32(i32, i32, i32)

define i32 @main() {
entry:
  %x = call i32 @llvm.fshl.i32(i32 1, i32 2, i32 3)
  ret i32 %x
}
