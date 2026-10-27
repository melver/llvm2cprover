; ---
; description = "non-splat <N x i1> constant vectors lower without crashing and with correct lanes"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; `i1` vector constants are a ConstantVector rather than a ConstantDataVector, so
; ConstantDataSequential::getElementAsConstant must not be used to read lanes.

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-linux-gnu"

declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %e0 = extractelement <2 x i1> <i1 true, i1 false>, i64 0
  %e1 = extractelement <2 x i1> <i1 true, i1 false>, i64 1
  %n1 = xor i1 %e1, true
  %ok = and i1 %e0, %n1
  call void @__CPROVER_assert(i1 %ok, ptr null)
  ret i32 0
}
