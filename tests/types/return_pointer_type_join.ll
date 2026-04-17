; ---
; description = "Return operands cast back to the lowered function pointer type"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare void @__CPROVER_assert(i1, ptr)

@.msg = private unnamed_addr constant [25 x i8] c"pointer return type join\00"

define ptr @program_new(ptr %0) {
entry:
  br i1 false, label %ret_null, label %ret_inttoptr

ret_null:
  ret ptr null

ret_inttoptr:
  %p = inttoptr i64 0 to ptr
  ret ptr %p
}

; This function is load-bearing for the regression: its `load i64, ptr null`
; causes the module-wide pointer-hint pass to tag the null constant as i64*,
; so the `ret ptr null` in program_new picks up a different pointee type than
; the `ret ptr %p` (inttoptr) site.  Without it both sites get the same type
; and the bug is hidden.
define fastcc i64 @register_function_hashed_legacy() {
entry:
  %x = load i64, ptr null, align 8
  ret i64 %x
}

define i32 @main() {
entry:
  %p = call ptr @program_new(ptr null)
  %ok = icmp eq ptr %p, null
  call void @__CPROVER_assert(i1 %ok, ptr @.msg)
  ret i32 0
}
