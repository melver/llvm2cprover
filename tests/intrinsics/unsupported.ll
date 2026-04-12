; ---
; description = "unsupported llvm intrinsics fail translation deterministically"
; expected = "TRANSLATION FAILED"
; entry = "main"
; ---

source_filename = "/tmp/unsupported_intrinsic.ll"

declare i64 @llvm.readcyclecounter()

define i32 @main() {
entry:
  %x = call i64 @llvm.readcyclecounter()
  %y = trunc i64 %x to i32
  ret i32 %y
}
