; ---
; description = "Loop back-edges retain PHI semantics"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; cbmc_flags = ["--unwind", "5"]
; ---

declare void @__CPROVER_assert(i1, ptr)

@.msg = private unnamed_addr constant [9 x i8] c"phi loop\00"

define i32 @main() {
entry:
  br label %loop

loop:
  %i = phi i32 [ 0, %entry ], [ %next, %body ]
  %sum = phi i32 [ 0, %entry ], [ %sum.next, %body ]
  %cond = icmp slt i32 %i, 4
  br i1 %cond, label %body, label %exit

body:
  %sum.next = add i32 %sum, %i
  %next = add i32 %i, 1
  br label %loop

exit:
  %ok = icmp eq i32 %sum, 6
  call void @__CPROVER_assert(i1 %ok, ptr @.msg)
  ret i32 0
}
