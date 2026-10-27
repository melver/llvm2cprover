; ---
; description = "<N x i1> add/mul compute lane-wise mod 2 (CBMC drops * and crashes on vector + over c_bool)"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-linux-gnu"
declare i32 @__CPROVER_nondet_int()
declare void @__CPROVER_assert(i1, ptr)
define i32 @main() {
entry:
  %x = call i32 @__CPROVER_nondet_int()
  %a = icmp sgt i32 %x, 0
  %va = insertelement <2 x i1> <i1 true, i1 false>, i1 %a, i64 1
  %s = add <2 x i1> %va, <i1 true, i1 true>
  %m = mul <2 x i1> %va, <i1 true, i1 false>
  %e0 = extractelement <2 x i1> %s, i64 0
  %e1 = extractelement <2 x i1> %s, i64 1
  %f0 = extractelement <2 x i1> %m, i64 0
  %f1 = extractelement <2 x i1> %m, i64 1
  %na = xor i1 %a, true
  %ok0 = icmp eq i1 %e0, false
  %ok1 = icmp eq i1 %e1, %na
  %ok2 = icmp eq i1 %f0, true
  %ok3 = icmp eq i1 %f1, false
  call void @__CPROVER_assert(i1 %ok0, ptr null)
  call void @__CPROVER_assert(i1 %ok1, ptr null)
  call void @__CPROVER_assert(i1 %ok2, ptr null)
  call void @__CPROVER_assert(i1 %ok3, ptr null)
  ret i32 0
}
