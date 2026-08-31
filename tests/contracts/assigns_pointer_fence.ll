; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
; ---
; expected = "TRANSLATION FAILED"
; description = "pointer-returning assigns helpers reject atomic fences"
; ---

@value = global i32 0

define i32 @target() {
entry:
  %result = load i32, ptr @value
  ret i32 %result
}

define ptr @assigns_target() {
entry:
  fence seq_cst
  ret ptr @value
}
