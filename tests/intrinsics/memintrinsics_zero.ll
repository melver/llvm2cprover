; ---
; description = "Zero-length LLVM memory intrinsics are treated as no-ops even with null pointers"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare void @llvm.memset.p0.i64(ptr writeonly, i8, i64, i1 immarg)
declare void @llvm.memcpy.p0.p0.i64(ptr noalias writeonly, ptr noalias readonly, i64, i1 immarg)
declare void @llvm.memmove.p0.p0.i64(ptr writeonly, ptr readonly, i64, i1 immarg)
declare void @__CPROVER_assert(i1 zeroext, ptr)

@.msg = private unnamed_addr constant [19 x i8] c"zero memintrinsics\00"

define i32 @main() {
entry:
  %buf = alloca [4 x i8], align 1
  %p0 = getelementptr inbounds [4 x i8], ptr %buf, i64 0, i64 0
  %p1 = getelementptr inbounds [4 x i8], ptr %buf, i64 0, i64 1
  store i8 17, ptr %p0, align 1
  store i8 34, ptr %p1, align 1

  call void @llvm.memset.p0.i64(ptr %buf, i8 99, i64 0, i1 false)
  call void @llvm.memset.p0.i64(ptr null, i8 77, i64 0, i1 false)
  call void @llvm.memcpy.p0.p0.i64(ptr %p1, ptr %p0, i64 0, i1 false)
  call void @llvm.memcpy.p0.p0.i64(ptr null, ptr null, i64 0, i1 false)
  call void @llvm.memmove.p0.p0.i64(ptr %p0, ptr %p1, i64 0, i1 false)
  call void @llvm.memmove.p0.p0.i64(ptr null, ptr null, i64 0, i1 false)

  %v0 = load i8, ptr %p0, align 1
  %ok0 = icmp eq i8 %v0, 17
  %v1 = load i8, ptr %p1, align 1
  %ok1 = icmp eq i8 %v1, 34
  %ok = and i1 %ok0, %ok1
  call void @__CPROVER_assert(i1 %ok, ptr @.msg)
  ret i32 0
}
