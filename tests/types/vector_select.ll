; ---
; description = "select with a vector condition picks each lane independently"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-linux-gnu"

@a = global i32 1
@b = global i32 2

declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %v = insertelement <2 x i32> <i32 0, i32 0>, i32 1, i64 0
  %m = icmp eq <2 x i32> %v, <i32 1, i32 1>            ; <1, 0>
  %s = select <2 x i1> %m, <2 x i32> <i32 10, i32 11>, <2 x i32> <i32 20, i32 21>
  %s0 = extractelement <2 x i32> %s, i64 0
  %s1 = extractelement <2 x i32> %s, i64 1
  %p = select <2 x i1> %m, <2 x ptr> <ptr @a, ptr @a>, <2 x ptr> <ptr @b, ptr @b>
  %p1 = extractelement <2 x ptr> %p, i64 1
  %x1 = load i32, ptr %p1, align 4
  %c0 = icmp eq i32 %s0, 10
  %c1 = icmp eq i32 %s1, 21
  %c2 = icmp eq i32 %x1, 2
  call void @__CPROVER_assert(i1 %c0, ptr null)
  call void @__CPROVER_assert(i1 %c1, ptr null)
  call void @__CPROVER_assert(i1 %c2, ptr null)
  ret i32 0
}
