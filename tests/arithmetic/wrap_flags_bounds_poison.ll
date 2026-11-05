; ---
; description = "A loop accumulator that grows keeps its nsw poison check"
; expected = "VERIFICATION FAILED"
; entry = "main"
; ---
;
; The phi grows by 2^29 per iteration, so its bound does not survive the fixpoint and the
; overflowing `add nsw` in the fourth iteration stays poison: its result need not be the wrapped
; value.

declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  br label %loop

loop:
  %i = phi i32 [ 0, %entry ], [ %i.next, %loop ]
  %acc = phi i32 [ 0, %entry ], [ %next, %loop ]
  %next = add nsw i32 %acc, 536870912
  %i.next = add nuw nsw i32 %i, 1
  %done = icmp eq i32 %i.next, 4
  br i1 %done, label %exit, label %loop

exit:
  %wrapped = icmp eq i32 %next, -2147483648
  call void @__CPROVER_assert(i1 %wrapped, ptr null)
  ret i32 0
}
