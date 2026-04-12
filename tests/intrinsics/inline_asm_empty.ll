; ---
; description = "empty and comment-only inline asm barriers lower as no-ops"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

@.msg = private unnamed_addr constant [4 x i8] c"asm\00"
@.msg2 = private unnamed_addr constant [8 x i8] c"comment\00"

declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  call void asm sideeffect "", "~{dirflag},~{fpsr},~{flags}"()
  %x = call i64 asm sideeffect "# compiler barrier", "=r,0,~{dirflag},~{fpsr},~{flags}"(i64 42)
  %ok = icmp eq i64 %x, 42
  call void @__CPROVER_assert(i1 true, ptr @.msg)
  call void @__CPROVER_assert(i1 %ok, ptr @.msg2)
  ret i32 0
}
