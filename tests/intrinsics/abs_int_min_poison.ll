; ---
; description = "llvm.abs of INT_MIN with is_int_min_poison yields an unconstrained result"
; expected = "VERIFICATION FAILED"
; entry = "main"
; ---

declare i32 @llvm.abs.i32(i32, i1)
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %p = call i32 @llvm.abs.i32(i32 -2147483648, i1 true)
  %ok = icmp eq i32 %p, -2147483648
  call void @__CPROVER_assert(i1 %ok, ptr null)
  ret i32 0
}
