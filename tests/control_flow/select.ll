; ---
; description = "LLVM select lowers as a value-level conditional expression"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare void @__CPROVER_assert(i1, ptr)

@.msg = private unnamed_addr constant [7 x i8] c"select\00"

define i32 @main() {
entry:
  %out = select i1 true, i32 9, i32 4
  %ok = icmp eq i32 %out, 9
  call void @__CPROVER_assert(i1 %ok, ptr @.msg)
  ret i32 0
}
