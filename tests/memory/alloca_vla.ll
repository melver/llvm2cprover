; ---
; description = "dynamic alloca lowers as an exact variable-length stack object"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

@.msg = private unnamed_addr constant [4 x i8] c"vla\00"

declare i64 @__CPROVER_nondet_ulong()
declare void @__CPROVER_assume(i1)
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %n = call i64 @__CPROVER_nondet_ulong()
  %i = call i64 @__CPROVER_nondet_ulong()
  %n_ok = icmp uge i64 %n, 1
  call void @__CPROVER_assume(i1 %n_ok)
  %i_ok = icmp ult i64 %i, %n
  call void @__CPROVER_assume(i1 %i_ok)

  %p = alloca i32, i64 %n, align 4
  %elt = getelementptr inbounds i32, ptr %p, i64 %i
  store i32 7, ptr %elt, align 4
  %v = load i32, ptr %elt, align 4
  %ok = icmp eq i32 %v, 7
  call void @__CPROVER_assert(i1 %ok, ptr @.msg)
  ret i32 0
}
