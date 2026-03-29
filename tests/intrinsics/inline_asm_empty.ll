; ---
; description = "empty inline asm barriers lower as no-ops"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

@.msg = private unnamed_addr constant [4 x i8] c"asm\00"

declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  call void asm sideeffect "", "~{dirflag},~{fpsr},~{flags}"()
  call void @__CPROVER_assert(i1 true, ptr @.msg)
  ret i32 0
}
