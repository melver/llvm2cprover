; ---
; description = "Truncating an integer to i1 extracts the least-significant bit, not non-zero truthiness"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare void @__CPROVER_assert(i1, ptr)

@.msg = private unnamed_addr constant [9 x i8] c"trunc i1\00"

define i32 @main() {
entry:
  %even = add i32 2, 0
  %even_bit = trunc i32 %even to i1
  %even_zext = zext i1 %even_bit to i32
  %even_ok = icmp eq i32 %even_zext, 0

  %odd = add i32 6, 1
  %odd_bit = trunc i32 %odd to i1
  %odd_sext = sext i1 %odd_bit to i32
  %odd_ok = icmp eq i32 %odd_sext, -1

  %not_even = xor i1 %even_bit, true
  %both = and i1 %even_ok, %odd_ok
  %ok = and i1 %both, %not_even
  call void @__CPROVER_assert(i1 %ok, ptr @.msg)
  ret i32 0
}
