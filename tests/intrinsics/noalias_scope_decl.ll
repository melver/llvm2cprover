; ---
; description = "llvm.experimental.noalias.scope.decl metadata intrinsic is ignored as a no-op"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare void @llvm.experimental.noalias.scope.decl(metadata)
declare void @__CPROVER_assert(i1, ptr)

@.msg = private unnamed_addr constant [13 x i8] c"noalias decl\00"

define i32 @main() {
entry:
  %x = alloca i32, align 4
  tail call void @llvm.experimental.noalias.scope.decl(metadata !0)
  store i32 42, ptr %x, align 4, !noalias !0
  %v = load i32, ptr %x, align 4, !alias.scope !0
  %ok = icmp eq i32 %v, 42
  call void @__CPROVER_assert(i1 %ok, ptr @.msg)
  ret i32 0
}

!0 = !{!1}
!1 = distinct !{!1, !2, !"callee: %p"}
!2 = distinct !{!2, !"callee"}
