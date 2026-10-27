; ---
; description = "llvm.vector.reduce.* horizontal reductions over fixed-width integer vectors"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; todo = "llvm.vector.reduce.{and,or,xor,add,mul,umin,umax,smin,smax} are unsupported intrinsics"
; ---
;
; The SLP vectorizer at -O2 turns Csmith's `g[0] & g[1] & g[2] & g[3]` over a
; 4-element global into `llvm.vector.reduce.and.v4i32` (seed 414). The whole
; family is a lane fold with a fixed operator and is absent from the
; intrinsic dispatcher, so translation fails closed. Vector element types
; are already supported (see vector_minmax.ll), so the fix is a generic
; left-fold over extracted lanes.

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-linux-gnu"

@g = global <4 x i32> <i32 -2, i32 7, i32 -1, i32 6>, align 16

declare i32 @llvm.vector.reduce.and.v4i32(<4 x i32>)
declare i32 @llvm.vector.reduce.or.v4i32(<4 x i32>)
declare i32 @llvm.vector.reduce.xor.v4i32(<4 x i32>)
declare i32 @llvm.vector.reduce.add.v4i32(<4 x i32>)
declare i32 @llvm.vector.reduce.mul.v4i32(<4 x i32>)
declare i32 @llvm.vector.reduce.umin.v4i32(<4 x i32>)
declare i32 @llvm.vector.reduce.umax.v4i32(<4 x i32>)
declare i32 @llvm.vector.reduce.smin.v4i32(<4 x i32>)
declare i32 @llvm.vector.reduce.smax.v4i32(<4 x i32>)
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %v = load <4 x i32>, ptr @g, align 16
  %and = call i32 @llvm.vector.reduce.and.v4i32(<4 x i32> %v)
  %or = call i32 @llvm.vector.reduce.or.v4i32(<4 x i32> %v)
  %xor = call i32 @llvm.vector.reduce.xor.v4i32(<4 x i32> %v)
  %add = call i32 @llvm.vector.reduce.add.v4i32(<4 x i32> %v)
  %mul = call i32 @llvm.vector.reduce.mul.v4i32(<4 x i32> %v)
  %umin = call i32 @llvm.vector.reduce.umin.v4i32(<4 x i32> %v)
  %umax = call i32 @llvm.vector.reduce.umax.v4i32(<4 x i32> %v)
  %smin = call i32 @llvm.vector.reduce.smin.v4i32(<4 x i32> %v)
  %smax = call i32 @llvm.vector.reduce.smax.v4i32(<4 x i32> %v)
  %c0 = icmp eq i32 %and, 6        ; -2 & 7 & -1 & 6
  %c1 = icmp eq i32 %or, -1
  %c2 = icmp eq i32 %xor, 0        ; (-2 ^ 7) ^ (-1 ^ 6)
  %c3 = icmp eq i32 %add, 10
  %c4 = icmp eq i32 %mul, 84
  %c5 = icmp eq i32 %umin, 6
  %c6 = icmp eq i32 %umax, -1
  %c7 = icmp eq i32 %smin, -2
  %c8 = icmp eq i32 %smax, 7
  %a0 = and i1 %c0, %c1
  %a1 = and i1 %a0, %c2
  %a2 = and i1 %a1, %c3
  %a3 = and i1 %a2, %c4
  %a4 = and i1 %a3, %c5
  %a5 = and i1 %a4, %c6
  %a6 = and i1 %a5, %c7
  %a7 = and i1 %a6, %c8
  call void @__CPROVER_assert(i1 %a7, ptr null)
  ret i32 0
}
