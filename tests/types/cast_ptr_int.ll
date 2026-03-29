; ---
; description = "Pointer/integer round-trips lower through ptrtoint and inttoptr"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare void @__CPROVER_assert(i1, ptr)

@value = global i32 9
@.msg = private unnamed_addr constant [8 x i8] c"ptr int\00"

define i32 @main() {
entry:
  %addr = ptrtoint ptr @value to i64
  %ptr = inttoptr i64 %addr to ptr
  %loaded = load i32, ptr %ptr, align 4
  %ok = icmp eq i32 %loaded, 9
  call void @__CPROVER_assert(i1 %ok, ptr @.msg)
  ret i32 0
}
