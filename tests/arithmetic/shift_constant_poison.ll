; ---
; description = "A constant shift count at or above the width is poison"
; expected = "VERIFICATION FAILED"
; entry = "main"
; ---

declare i32 @__CPROVER_nondet_int()
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %x = call i32 @__CPROVER_nondet_int()
  %a = shl i32 %x, 32
  %b = shl i32 %x, 32
  %same = icmp eq i32 %a, %b
  call void @__CPROVER_assert(i1 %same, ptr null)
  ret i32 0
}
