; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
; ---
; expected = "TRANSLATION FAILED"
; description = "pointer-returning assigns helpers must be compiled definitions"
; ---

define i32 @target() {
entry:
  ret i32 0
}

declare ptr @assigns_target()
