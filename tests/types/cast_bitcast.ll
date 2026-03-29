; ---
; description = "Bitcasts between same-size pointer types preserve the address"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare void @__CPROVER_assert(i1, ptr)

@value = global i32 7
@.msg = private unnamed_addr constant [8 x i8] c"bitcast\00"

define i32 @main() {
entry:
  %ptr = bitcast ptr @value to ptr
  %loaded = load i32, ptr %ptr, align 4
  %ok = icmp eq i32 %loaded, 7
  call void @__CPROVER_assert(i1 %ok, ptr @.msg)
  ret i32 0
}
