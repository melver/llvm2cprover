; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "a manifest model stands in for an unsupported or supported LLVM intrinsic"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; `pmovmskb` has no built-in lowering and would fail closed without its model. The
; `ctpop` model deliberately disagrees with LLVM to show an explicit model takes
; precedence over the built-in lowering.

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"

declare void @__CPROVER_assert(i1, ptr)
declare i32 @llvm.x86.sse2.pmovmskb.128(<16 x i8>)
declare i32 @llvm.ctpop.i32(i32)

define i32 @model_pmovmskb128(<16 x i8> %v) {
entry:
  %sign = icmp slt <16 x i8> %v, zeroinitializer
  %bits = bitcast <16 x i1> %sign to i16
  %mask = zext i16 %bits to i32
  ret i32 %mask
}

define i32 @model_ctpop(i32 %x) {
entry:
  ret i32 7
}

define i32 @main() {
entry:
  %mask = call i32 @llvm.x86.sse2.pmovmskb.128(<16 x i8> <i8 0, i8 -128, i8 1, i8 -1, i8 0, i8 0, i8 0, i8 0, i8 0, i8 0, i8 0, i8 0, i8 0, i8 0, i8 0, i8 127>)
  %mask_ok = icmp eq i32 %mask, 10
  call void @__CPROVER_assert(i1 %mask_ok, ptr null)
  %pop = call i32 @llvm.ctpop.i32(i32 0)
  %pop_ok = icmp eq i32 %pop, 7
  call void @__CPROVER_assert(i1 %pop_ok, ptr null)
  ret i32 0
}
