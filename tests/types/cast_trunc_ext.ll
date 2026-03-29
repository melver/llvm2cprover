; ---
; description = "Integer truncation and extension lower through cast instructions"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare void @__CPROVER_assert(i1, ptr)

@.msg = private unnamed_addr constant [5 x i8] c"cast\00"

define i32 @main() {
entry:
  %wide = add i32 255, 1
  %narrow = trunc i32 %wide to i8
  %back = zext i8 %narrow to i32
  %ok = icmp eq i32 %back, 0
  call void @__CPROVER_assert(i1 %ok, ptr @.msg)
  ret i32 0
}
