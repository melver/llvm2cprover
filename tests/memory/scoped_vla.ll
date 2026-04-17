; ---
; description = "scoped VLA with llvm.stacksave/llvm.stackrestore lowers correctly"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare ptr @llvm.stacksave()
declare void @llvm.stackrestore(ptr)

declare i64 @__CPROVER_nondet_ulong()
declare void @__CPROVER_assume(i1)
declare void @__CPROVER_assert(i1, ptr)

@.msg = private unnamed_addr constant [3 x i8] c"ok\00"

define i32 @main() {
entry:
  %n = call i64 @__CPROVER_nondet_ulong()
  %n_ok = icmp uge i64 %n, 1
  call void @__CPROVER_assume(i1 %n_ok)
  %n_bound = icmp ule i64 %n, 16
  call void @__CPROVER_assume(i1 %n_bound)

  ; scoped VLA: C compiler emits stacksave before the alloca
  %sp = call ptr @llvm.stacksave()
  %vla = alloca i32, i64 %n, align 4
  store i32 42, ptr %vla, align 4
  %v = load i32, ptr %vla, align 4
  %ok = icmp eq i32 %v, 42
  call void @__CPROVER_assert(i1 %ok, ptr @.msg)
  ; end of scope: stackrestore frees the VLA
  call void @llvm.stackrestore(ptr %sp)

  ret i32 0
}
