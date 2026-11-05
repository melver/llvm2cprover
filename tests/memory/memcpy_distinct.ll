; ---
; description = "llvm.memcpy between distinct objects (lowered without the self-copy guard) copies exactly"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare i32 @__CPROVER_nondet_int()
declare void @__CPROVER_assert(i1, ptr)
declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)

@g = global [4 x i32] [i32 1, i32 2, i32 3, i32 4]

define i32 @main() {
entry:
  %x = call i32 @__CPROVER_nondet_int()
  %a = alloca [4 x i32]
  %b = alloca [4 x i32]
  call void @llvm.memcpy.p0.p0.i64(ptr %a, ptr @g, i64 16, i1 false)
  %a1 = getelementptr i32, ptr %a, i64 1
  store i32 %x, ptr %a1
  call void @llvm.memcpy.p0.p0.i64(ptr %b, ptr %a, i64 16, i1 false)
  %g2 = getelementptr i8, ptr @g, i64 8
  call void @llvm.memcpy.p0.p0.i64(ptr %g2, ptr %b, i64 8, i1 false)
  %b0 = load i32, ptr %b
  %b1 = getelementptr i32, ptr %b, i64 1
  %v1 = load i32, ptr %b1
  %g3 = getelementptr i32, ptr @g, i64 3
  %v3 = load i32, ptr %g3
  %ok0 = icmp eq i32 %b0, 1
  %ok1 = icmp eq i32 %v1, %x
  %ok3 = icmp eq i32 %v3, %x
  call void @__CPROVER_assert(i1 %ok0, ptr null)
  call void @__CPROVER_assert(i1 %ok1, ptr null)
  call void @__CPROVER_assert(i1 %ok3, ptr null)
  ret i32 0
}
