; ---
; description = "llvm min/max intrinsics lower as compare-select"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

@.msg = private unnamed_addr constant [7 x i8] c"minmax\00"

declare void @__CPROVER_assert(i1, ptr)
declare i32 @llvm.umin.i32(i32, i32)
declare i32 @llvm.umax.i32(i32, i32)
declare i32 @llvm.smin.i32(i32, i32)
declare i32 @llvm.smax.i32(i32, i32)
declare i128 @llvm.umin.i128(i128, i128)

define i32 @main() {
entry:
  %umin = call i32 @llvm.umin.i32(i32 7, i32 4)
  %umax = call i32 @llvm.umax.i32(i32 7, i32 4)
  %smin = call i32 @llvm.smin.i32(i32 -3, i32 2)
  %smax = call i32 @llvm.smax.i32(i32 -3, i32 2)
  %wide = call i128 @llvm.umin.i128(i128 18446744073709551616, i128 18446744073709551615)

  %ok1 = icmp eq i32 %umin, 4
  %ok2 = icmp eq i32 %umax, 7
  %ok3 = icmp eq i32 %smin, -3
  %ok4 = icmp eq i32 %smax, 2
  %ok5 = icmp eq i128 %wide, 18446744073709551615
  %ok12 = and i1 %ok1, %ok2
  %ok34 = and i1 %ok3, %ok4
  %ok1234 = and i1 %ok12, %ok34
  %ok = and i1 %ok1234, %ok5
  call void @__CPROVER_assert(i1 %ok, ptr @.msg)
  ret i32 0
}
