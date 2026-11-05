; ---
; description = "Constant shift counts below the width are exact without a count guard"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare i32 @__CPROVER_nondet_int()
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %x = call i32 @__CPROVER_nondet_int()
  %l = shl i32 %x, 4
  %m = mul i32 %x, 16
  %ok = icmp eq i32 %l, %m
  call void @__CPROVER_assert(i1 %ok, ptr null)
  %r = lshr i32 %l, 4
  %k = and i32 %x, 268435455
  %ok2 = icmp eq i32 %r, %k
  call void @__CPROVER_assert(i1 %ok2, ptr null)
  %s = ashr i32 %x, 31
  %neg = icmp slt i32 %x, 0
  %e = sext i1 %neg to i32
  %ok3 = icmp eq i32 %s, %e
  call void @__CPROVER_assert(i1 %ok3, ptr null)
  ret i32 0
}
