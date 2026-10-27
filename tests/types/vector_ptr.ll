; ---
; description = "<N x ptr> vectors lower as pointer arrays and support store/load/lane access/casts/icmp/phi/select/struct fields"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; The SLP vectorizer at -O2 stores pointer splats (Csmith seeds 1197, 3082, 3625,
; 4023, 4936), which used to panic in type lowering.

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-linux-gnu"

@a = global i32 1, align 4
@b = global [2 x i32] [i32 2, i32 3], align 4
@slots = global [2 x ptr] zeroinitializer, align 16
@pair = global { i64, <2 x ptr> } zeroinitializer, align 16

declare void @__CPROVER_assert(i1, ptr)

define internal ptr @second(<2 x ptr> %v) {
entry:
  %e = extractelement <2 x ptr> %v, i64 1
  ret ptr %e
}

define i32 @main() {
entry:
  store <2 x ptr> <ptr @a, ptr getelementptr inbounds nuw (i8, ptr @b, i64 4)>, ptr @slots, align 16
  %p1 = getelementptr inbounds [2 x ptr], ptr @slots, i64 0, i64 1
  %q1 = load ptr, ptr %p1, align 8
  %x1 = load i32, ptr %q1, align 4
  %c0 = icmp eq i32 %x1, 3

  %v = load <2 x ptr>, ptr @slots, align 16
  %w = insertelement <2 x ptr> %v, ptr @b, i64 1
  %s = shufflevector <2 x ptr> %w, <2 x ptr> poison, <2 x i32> <i32 1, i32 0>
  %si = ptrtoint <2 x ptr> %s to <2 x i64>
  %sp = inttoptr <2 x i64> %si to <2 x ptr>
  %q0 = call ptr @second(<2 x ptr> %sp)
  %x0 = load i32, ptr %q0, align 4
  %c1 = icmp eq i32 %x0, 1

  %m = icmp eq <2 x ptr> %s, <ptr @b, ptr @b>
  %m0 = extractelement <2 x i1> %m, i64 0
  %m1 = extractelement <2 x i1> %m, i64 1
  %nm1 = xor i1 %m1, true
  %c2 = and i1 %m0, %nm1

  ; A <2 x ptr> struct field sits at its LLVM offset (16), not void*[2] packing.
  %f = getelementptr inbounds { i64, <2 x ptr> }, ptr @pair, i64 0, i32 1
  store <2 x ptr> %s, ptr %f, align 16
  %raw = getelementptr inbounds i8, ptr @pair, i64 24
  %q2 = load ptr, ptr %raw, align 8
  %c3 = icmp eq ptr %q2, @a
  %fr = freeze <2 x ptr> %s
  br i1 %c3, label %then, label %join

then:
  %sel = select i1 %c3, <2 x ptr> %fr, <2 x ptr> zeroinitializer
  br label %join

join:
  %ph = phi <2 x ptr> [ %sel, %then ], [ zeroinitializer, %entry ]
  %q3 = call ptr @second(<2 x ptr> %ph)
  %c4 = icmp eq ptr %q3, @a

  call void @__CPROVER_assert(i1 %c0, ptr null)
  call void @__CPROVER_assert(i1 %c1, ptr null)
  call void @__CPROVER_assert(i1 %c2, ptr null)
  call void @__CPROVER_assert(i1 %c3, ptr null)
  call void @__CPROVER_assert(i1 %c4, ptr null)
  ret i32 0
}
