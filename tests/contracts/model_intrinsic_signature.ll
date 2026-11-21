; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "an intrinsic model must match the intrinsic's signature"
; expected = "TRANSLATION FAILED"
; entry = "main"
; ---

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"

declare i32 @llvm.x86.sse2.pmovmskb.128(<16 x i8>)

define i64 @model_pmovmskb128(<16 x i8> %v) {
entry:
  ret i64 0
}

define i32 @main() {
entry:
  %mask = call i32 @llvm.x86.sse2.pmovmskb.128(<16 x i8> zeroinitializer)
  ret i32 %mask
}
