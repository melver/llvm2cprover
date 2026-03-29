; ---
; description = "llvm.cttz lowers to CBMC trailing-zero counting"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

@.msg = private unnamed_addr constant [5 x i8] c"cttz\00"

declare i32 @llvm.cttz.i32(i32, i1)
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %count = call i32 @llvm.cttz.i32(i32 8, i1 true)
  %ok = icmp eq i32 %count, 3
  call void @__CPROVER_assert(i1 %ok, ptr @.msg)
  ret i32 0
}
