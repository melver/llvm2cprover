; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "direct call through a cross-type GlobalAlias with incompatible aggregate signature fails closed without panicking"
; expected = "TRANSLATION FAILED"
; entry = "main"
; ---

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"

%struct.A = type { i64 }
%struct.B = type { ptr }

@as_a = unnamed_addr alias %struct.A (%struct.A), ptr @id_b

define %struct.B @id_b(%struct.B %x) {
entry:
  ret %struct.B %x
}

define i32 @main() {
entry:
  %r = call %struct.A @as_a(%struct.A { i64 7 })
  ret i32 0
}
