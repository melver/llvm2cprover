; ---
; description = "llvm.compiler.used metadata globals are ignored during translation"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

@llvm.compiler.used = appending global [1 x ptr] [ptr @main], section "llvm.metadata"

declare void @__CPROVER_assert(i1, ptr)

@.msg = private unnamed_addr constant [14 x i8] c"compiler.used\00"

define i32 @main() {
entry:
  call void @__CPROVER_assert(i1 true, ptr @.msg)
  ret i32 0
}
