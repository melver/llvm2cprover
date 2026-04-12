; ---
; description = "Block lowering order follows CFG dominance, not textual block order"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare void @__CPROVER_assert(i1, ptr)

@.msg = private unnamed_addr constant [18 x i8] c"phi dominator rpo\00"

define i32 @main() {
entry:
  br i1 true, label %dom, label %join

join:
  %x = phi i32 [ 0, %entry ], [ %v, %pred ]
  %ok = icmp eq i32 %x, 7
  call void @__CPROVER_assert(i1 %ok, ptr @.msg)
  ret i32 0

pred:
  br label %join

dom:
  %v = add i32 3, 4
  br label %pred
}
