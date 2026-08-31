; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
; ---
; expected = "TRANSLATION FAILED"
; description = "assigns helpers reject local stack frames"
; ---

define i32 @target() {
entry:
  ret i32 0
}

define ptr @assigns_target() {
entry:
  %local = alloca i32
  %derived = getelementptr i32, ptr %local, i64 0
  ret ptr %derived
}
