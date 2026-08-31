; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "Inline asm models cannot collide with predeclared runtime symbols"
; expected = "TRANSLATION FAILED"
; ---

define i32 @abort(i32 %value) {
entry:
  ret i32 %value
}

define i32 @main() {
entry:
  %result = call i32 asm sideeffect "opaque", "=r,r"(i32 1)
  ret i32 %result
}
