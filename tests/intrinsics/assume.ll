; ---
; description = "llvm.assume lowers to a CBMC assumption"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare void @llvm.assume(i1)
declare void @__CPROVER_assert(i1, ptr)

@.msg = private unnamed_addr constant [7 x i8] c"assume\00"

define i32 @main() {
entry:
  call void @llvm.assume(i1 true)
  call void @__CPROVER_assert(i1 true, ptr @.msg)
  ret i32 0
}
