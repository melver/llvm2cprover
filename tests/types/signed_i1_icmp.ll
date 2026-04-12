; ---
; description = "Signed i1 compares reinterpret lowered bools as one-bit signed integers"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare void @__CPROVER_assert(i1, ptr)

@.msg = private unnamed_addr constant [24 x i8] c"signed i1 compare works\00"

define i1 @signed_lt(i1 %lhs, i1 %rhs) {
entry:
  %cmp = icmp slt i1 %lhs, %rhs
  ret i1 %cmp
}

define i32 @main() {
entry:
  %true_lt_false = call i1 @signed_lt(i1 true, i1 false)
  %false_lt_true = call i1 @signed_lt(i1 false, i1 true)
  %not_false_lt_true = xor i1 %false_lt_true, true
  %ok = and i1 %true_lt_false, %not_false_lt_true
  call void @__CPROVER_assert(i1 %ok, ptr @.msg)
  ret i32 0
}
