; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "contract replacement peels GlobalAlias wrappers and replaces direct alias calls"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; translator_flags = ["--replace-with-contract", "target"]
; ---

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"

@body_ran = global i32 0
@unused_alias = unnamed_addr alias i32 (i32), ptr @target
@call_alias = unnamed_addr alias i32 (i32), ptr @target

declare void @__CPROVER_assert(i1, ptr)

define i32 @target(i32 %x) {
entry:
  store i32 1, ptr @body_ran, align 4
  %bad = sub i32 %x, 1
  ret i32 %bad
}

define i1 @ensures_target(i32 %x, i32 %result) {
entry:
  %expected = add i32 %x, 1
  %ok = icmp eq i32 %result, %expected
  ret i1 %ok
}

define i32 @main() {
entry:
  %res = call i32 @call_alias(i32 4)
  %ok_res = icmp eq i32 %res, 5
  call void @__CPROVER_assert(i1 %ok_res, ptr null)
  %ran = load i32, ptr @body_ran, align 4
  %ok_bypass = icmp eq i32 %ran, 0
  call void @__CPROVER_assert(i1 %ok_bypass, ptr null)
  ret i32 0
}
