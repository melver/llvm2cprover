; ---
; description = "Diamond control flow retains PHI merge semantics"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare void @__CPROVER_assert(i1, ptr)

@.msg = private unnamed_addr constant [4 x i8] c"phi\00"

define i32 @pick(i1 %cond) {
entry:
  br i1 %cond, label %left, label %right

left:
  br label %merge

right:
  br label %merge

merge:
  %out = phi i32 [ 11, %left ], [ 22, %right ]
  ret i32 %out
}

define i32 @main() {
entry:
  %out = call i32 @pick(i1 true)
  %ok = icmp eq i32 %out, 11
  call void @__CPROVER_assert(i1 %ok, ptr @.msg)
  ret i32 0
}
