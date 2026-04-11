; ---
; description = "llvm freeze lowers as identity on defined values"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

@.msg = private unnamed_addr constant [7 x i8] c"freeze\00"

declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %seed = add i32 5, 2
  %x = freeze i32 %seed
  %ok = icmp eq i32 %x, 7
  br i1 %ok, label %pass, label %fail

pass:
  call void @__CPROVER_assert(i1 true, ptr @.msg)
  ret i32 0

fail:
  call void @__CPROVER_assert(i1 false, ptr @.msg)
  ret i32 0
}
