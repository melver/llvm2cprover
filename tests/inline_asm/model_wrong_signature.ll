; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "Inline asm model signature mismatches fail closed"
; expected = "TRANSLATION FAILED"
; ---

define i64 @wrong_model(i32 %value) {
entry:
  %wide = zext i32 %value to i64
  ret i64 %wide
}

define i32 @main() {
entry:
  %result = call i32 asm sideeffect "opaque", "=r,r"(i32 1)
  ret i32 %result
}
