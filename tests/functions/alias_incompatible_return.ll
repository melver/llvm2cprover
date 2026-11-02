; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "direct call through a cross-type GlobalAlias with non-equivalent scalar return fails closed instead of converting numerically"
; expected = "TRANSLATION FAILED"
; entry = "main"
; ---

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"

@as_i64 = unnamed_addr alias i64 (), ptr @returns_double

define double @returns_double() {
entry:
  ret double 1.5
}

define i32 @main() {
entry:
  %r = call i64 @as_i64()
  ret i32 0
}
