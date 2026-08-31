; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "Inline asm model helpers must be defined in the input module"
; expected = "TRANSLATION FAILED"
; ---

declare i32 @missing_model(i32)

define i32 @main() {
entry:
  %result = call i32 asm sideeffect "opaque", "=r,r"(i32 1)
  ret i32 %result
}
