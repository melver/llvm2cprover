; ---
; description = "undef constants in function bodies lower to nondet values"
; expected = "VERIFICATION FAILED"
; entry = "main"
; ---
;
; LLVM undef means "any bit-pattern at each use". In a function body llvm2cprover
; maps it to Expr::nondet so CBMC explores all possible values.

@.msg = private unnamed_addr constant [11 x i8] c"undef zero\00"

declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %val = add i32 undef, 0
  ; CBMC explores val != 0 and falsifies the assertion.
  %ok = icmp eq i32 %val, 0
  call void @__CPROVER_assert(i1 %ok, ptr @.msg)
  ret i32 0
}
