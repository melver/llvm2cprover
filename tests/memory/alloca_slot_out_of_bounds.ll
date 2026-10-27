; ---
; description = "out-of-range GEP into a counted alloca must be reported even when only pointer checks are enabled"
; expected = "VERIFICATION FAILED"
; entry = "main"
; cbmc_flags = ["--no-standard-checks", "--pointer-check"]
; ---

declare i64 @__CPROVER_nondet_ulong()
declare void @__CPROVER_assume(i1)

define i32 @main() {
entry:
  %i = call i64 @__CPROVER_nondet_ulong()
  %i_ok = icmp ule i64 %i, 4
  call void @__CPROVER_assume(i1 %i_ok)
  %p = alloca i8, i64 4, align 1
  %elt = getelementptr i8, ptr %p, i64 %i
  store i8 7, ptr %elt, align 1
  %v = load i8, ptr %elt, align 1
  ret i32 0
}
