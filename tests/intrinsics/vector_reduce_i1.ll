; ---
; description = "llvm.vector.reduce.{and,or,xor,add,mul,umin,umax,smin,smax} over <4 x i1> mask vectors"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; Vectorizers emit `reduce.or`/`reduce.and` over `icmp` masks for any-of/all-of
; loops. Lanes are `i1`: `add` wraps (== xor), `mul` == and, and signed `i1` is {0, -1}, so
; `smin` behaves like `or` and `smax` like `and`.

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-linux-gnu"

@g = global <4 x i32> <i32 -2, i32 7, i32 -1, i32 6>, align 16

declare i1 @llvm.vector.reduce.and.v4i1(<4 x i1>)
declare i1 @llvm.vector.reduce.or.v4i1(<4 x i1>)
declare i1 @llvm.vector.reduce.xor.v4i1(<4 x i1>)
declare i1 @llvm.vector.reduce.add.v4i1(<4 x i1>)
declare i1 @llvm.vector.reduce.mul.v4i1(<4 x i1>)
declare i1 @llvm.vector.reduce.umin.v4i1(<4 x i1>)
declare i1 @llvm.vector.reduce.umax.v4i1(<4 x i1>)
declare i1 @llvm.vector.reduce.smin.v4i1(<4 x i1>)
declare i1 @llvm.vector.reduce.smax.v4i1(<4 x i1>)
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %v = load <4 x i32>, ptr @g, align 16
  %m = icmp sgt <4 x i32> %v, zeroinitializer   ; <0, 1, 0, 1>
  %and = call i1 @llvm.vector.reduce.and.v4i1(<4 x i1> %m)
  %or = call i1 @llvm.vector.reduce.or.v4i1(<4 x i1> %m)
  %xor = call i1 @llvm.vector.reduce.xor.v4i1(<4 x i1> %m)
  %add = call i1 @llvm.vector.reduce.add.v4i1(<4 x i1> %m)
  %mul = call i1 @llvm.vector.reduce.mul.v4i1(<4 x i1> %m)
  %umin = call i1 @llvm.vector.reduce.umin.v4i1(<4 x i1> %m)
  %umax = call i1 @llvm.vector.reduce.umax.v4i1(<4 x i1> %m)
  %smin = call i1 @llvm.vector.reduce.smin.v4i1(<4 x i1> %m)
  %smax = call i1 @llvm.vector.reduce.smax.v4i1(<4 x i1> %m)
  %c0 = xor i1 %and, true          ; and  == 0
  %c1 = and i1 %c0, %or            ; or   == 1
  %n2 = xor i1 %xor, true          ; xor  == 0
  %c2 = and i1 %c1, %n2
  %n3 = xor i1 %add, true          ; add  == 0
  %c3 = and i1 %c2, %n3
  %n4 = xor i1 %umin, true         ; umin == 0
  %c4 = and i1 %c3, %n4
  %c5 = and i1 %c4, %umax          ; umax == 1
  %c6 = and i1 %c5, %smin          ; smin == 1 (-1)
  %n7 = xor i1 %smax, true         ; smax == 0
  %c7 = and i1 %c6, %n7
  %n8 = xor i1 %mul, true          ; mul  == 0
  %c8 = and i1 %c7, %n8
  call void @__CPROVER_assert(i1 %c8, ptr null)
  ret i32 0
}
