; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "a replaced call's unconstrained result keeps its i1 fields 0 or 1"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; translator_flags = ["--replace-with-contract", "target"]
; ---
;
; Without an ensures clause the replacement result is nondet; CBMC would leave
; a c_bool field of an aggregate nondet unconstrained, so an i1 could be true
; for a branch and differ from `true` for icmp/xor.

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"

declare void @__CPROVER_assert(i1, ptr)

define { i32, i1 } @target(i32 %x) {
entry:
  ret { i32, i1 } { i32 0, i1 false }
}

define i32 @main() {
entry:
  %p = call { i32, i1 } @target(i32 4)
  %b = extractvalue { i32, i1 } %p, 1
  br i1 %b, label %set, label %done
set:
  %not = xor i1 %b, true
  %ok = icmp eq i1 %not, false
  call void @__CPROVER_assert(i1 %ok, ptr null)
  br label %done
done:
  ret i32 0
}
