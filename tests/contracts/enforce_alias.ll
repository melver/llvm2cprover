; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "contract enforcement allows targets with unused and direct-call-only GlobalAlias wrappers"
; expected = "VERIFICATION SUCCESSFUL"
; translator_flags = ["--enforce-contract", "target"]
; ---

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"

@unused_alias = unnamed_addr alias i32 (i32), ptr @target
@call_alias = unnamed_addr alias i32 (i32), ptr @target

define i32 @target(i32 %x) {
entry:
  %r = add i32 %x, 1
  ret i32 %r
}

define i1 @ensures_target(i32 %x, i32 %result) {
entry:
  %expected = add i32 %x, 1
  %ok = icmp eq i32 %result, %expected
  ret i1 %ok
}

define i32 @caller(i32 %x) {
entry:
  %r = call i32 @call_alias(i32 %x)
  ret i32 %r
}
