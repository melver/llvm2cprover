; ---
; description = "Vector sdiv/srem compute lane-wise in signed arithmetic"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-linux-gnu"
declare i8 @__CPROVER_nondet_char()
declare void @__CPROVER_assert(i1, ptr)
define i32 @main() {
entry:
  %x = call i8 @__CPROVER_nondet_char()
  %v = insertelement <2 x i8> <i8 -7, i8 0>, i8 %x, i64 1
  %q = sdiv <2 x i8> %v, <i8 2, i8 -3>
  %r = srem <2 x i8> %v, <i8 2, i8 -3>
  %q0 = extractelement <2 x i8> %q, i64 0
  %r0 = extractelement <2 x i8> %r, i64 0
  %q1 = extractelement <2 x i8> %q, i64 1
  %r1 = extractelement <2 x i8> %r, i64 1
  %ok0 = icmp eq i8 %q0, -3
  %ok1 = icmp eq i8 %r0, -1
  %m = mul i8 %q1, -3
  %back = add i8 %m, %r1
  %ok2 = icmp eq i8 %back, %x
  %bq = sdiv <2 x i1> <i1 false, i1 false>, <i1 true, i1 true>
  %br = srem <2 x i1> <i1 false, i1 false>, <i1 true, i1 true>
  %bq0 = extractelement <2 x i1> %bq, i64 0
  %bq1 = extractelement <2 x i1> %bq, i64 1
  %br0 = extractelement <2 x i1> %br, i64 0
  %br1 = extractelement <2 x i1> %br, i64 1
  %b01 = or i1 %bq0, %bq1
  %b23 = or i1 %br0, %br1
  %bany = or i1 %b01, %b23
  %ok3 = xor i1 %bany, true
  call void @__CPROVER_assert(i1 %ok0, ptr null)
  call void @__CPROVER_assert(i1 %ok1, ptr null)
  call void @__CPROVER_assert(i1 %ok2, ptr null)
  call void @__CPROVER_assert(i1 %ok3, ptr null)
  ret i32 0
}
