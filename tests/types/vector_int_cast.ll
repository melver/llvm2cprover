; ---
; description = "zext/sext/trunc on integer vectors convert each lane"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; Csmith seed 4759 (-O2) widens a vector icmp mask with `zext <2 x i1> to <2 x i32>`.

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-linux-gnu"

declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %v = insertelement <2 x i32> <i32 0, i32 -1>, i32 300, i64 0
  %m = icmp eq <2 x i32> %v, <i32 300, i32 0>
  %z1 = zext <2 x i1> %m to <2 x i32>
  %s1 = sext <2 x i1> %m to <2 x i32>
  %t8 = trunc <2 x i32> %v to <2 x i8>
  %z8 = zext <2 x i8> %t8 to <2 x i32>
  %s8 = sext <2 x i8> %t8 to <2 x i32>
  %z1a = extractelement <2 x i32> %z1, i64 0
  %z1b = extractelement <2 x i32> %z1, i64 1
  %s1a = extractelement <2 x i32> %s1, i64 0
  %z8a = extractelement <2 x i32> %z8, i64 0
  %z8b = extractelement <2 x i32> %z8, i64 1
  %s8b = extractelement <2 x i32> %s8, i64 1
  %t1 = trunc <2 x i32> <i32 2, i32 3> to <2 x i1>   ; low bit, not low byte
  %t1a = extractelement <2 x i1> %t1, i64 0
  %t1b = extractelement <2 x i1> %t1, i64 1
  %c0 = icmp eq i32 %z1a, 1
  %c1 = icmp eq i32 %z1b, 0
  %c2 = icmp eq i32 %s1a, -1
  %c3 = icmp eq i32 %z8a, 44          ; 300 mod 256
  %c4 = icmp eq i32 %z8b, 255
  %c5 = icmp eq i32 %s8b, -1
  %n6 = xor i1 %t1a, true
  %c6 = and i1 %n6, %t1b
  call void @__CPROVER_assert(i1 %c0, ptr null)
  call void @__CPROVER_assert(i1 %c1, ptr null)
  call void @__CPROVER_assert(i1 %c2, ptr null)
  call void @__CPROVER_assert(i1 %c3, ptr null)
  call void @__CPROVER_assert(i1 %c4, ptr null)
  call void @__CPROVER_assert(i1 %c5, ptr null)
  call void @__CPROVER_assert(i1 %c6, ptr null)
  ret i32 0
}
