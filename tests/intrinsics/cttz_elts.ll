; ---
; description = "llvm.experimental.cttz.elts counts the trailing zero lanes of an integer vector"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare i64 @llvm.experimental.cttz.elts.i64.v16i1(<16 x i1>, i1)
declare i32 @llvm.experimental.cttz.elts.i32.v4i1(<4 x i1>, i1)
declare i1 @llvm.vector.reduce.or.v16i1(<16 x i1>)
declare i8 @llvm.experimental.cttz.elts.i8.v4i32(<4 x i32>, i1)
declare i1 @llvm.experimental.cttz.elts.i1.v1i1(<1 x i1>, i1)
declare i16 @nondet_bits()
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  ; The first true lane is the count; an all-false mask yields the lane count.
  %c = call i32 @llvm.experimental.cttz.elts.i32.v4i1(<4 x i1> <i1 0, i1 0, i1 1, i1 1>, i1 false)
  %c_ok = icmp eq i32 %c, 2
  call void @__CPROVER_assert(i1 %c_ok, ptr null)
  %z = call i32 @llvm.experimental.cttz.elts.i32.v4i1(<4 x i1> zeroinitializer, i1 false)
  %z_ok = icmp eq i32 %z, 4
  call void @__CPROVER_assert(i1 %z_ok, ptr null)
  ; Wider lanes count as zero or nonzero; a one-lane vector fits an i1 result.
  %w = call i8 @llvm.experimental.cttz.elts.i8.v4i32(<4 x i32> <i32 0, i32 0, i32 0, i32 -8>, i1 false)
  %w_ok = icmp eq i8 %w, 3
  call void @__CPROVER_assert(i1 %w_ok, ptr null)
  %one = call i1 @llvm.experimental.cttz.elts.i1.v1i1(<1 x i1> zeroinitializer, i1 false)
  call void @__CPROVER_assert(i1 %one, ptr null)

  ; For any mask: the count is at most 16, 16 exactly when no lane is set, else a set lane.
  %bits = call i16 @nondet_bits()
  %m = bitcast i16 %bits to <16 x i1>
  %n = call i64 @llvm.experimental.cttz.elts.i64.v16i1(<16 x i1> %m, i1 false)
  %any = call i1 @llvm.vector.reduce.or.v16i1(<16 x i1> %m)
  %le = icmp ule i64 %n, 16
  call void @__CPROVER_assert(i1 %le, ptr null)
  %full = icmp eq i64 %n, 16
  %none = xor i1 %any, true
  %full_ok = icmp eq i1 %full, %none
  call void @__CPROVER_assert(i1 %full_ok, ptr null)
  br i1 %any, label %set, label %done
set:
  %lane = extractelement <16 x i1> %m, i64 %n
  call void @__CPROVER_assert(i1 %lane, ptr null)
  br label %done
done:
  ret i32 0
}
