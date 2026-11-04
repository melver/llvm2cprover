; ---
; description = "Nested loops with symbolic bounds stay nested in the GOTO program, so each loop's unwinding is counted separately"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; cbmc_flags = ["--unwind", "4", "--unwinding-assertions"]
; ---

; Outer loop runs n <= 3 times, inner m <= 3 times, blocks in nesting order.
; Bug: RPO block lowering puts outer.latch (with the outer backward GOTO)
; between inner.header and inner.body; the loops then overlap and CBMC counts
; inner iterations against the outer loop: main.unwind.0 fails after one outer
; iteration with m = 3, at any --unwind.

declare i32 @__CPROVER_nondet_int()
declare void @__CPROVER_assume(i1 zeroext)

define i32 @main() {
entry:
  %n = call i32 @__CPROVER_nondet_int()
  %m = call i32 @__CPROVER_nondet_int()
  %n.ok = icmp ule i32 %n, 3
  %m.ok = icmp ule i32 %m, 3
  call void @__CPROVER_assume(i1 zeroext %n.ok)
  call void @__CPROVER_assume(i1 zeroext %m.ok)
  %has = icmp sgt i32 %n, 0
  br i1 %has, label %outer.header, label %exit

outer.header:
  %j = phi i32 [ 0, %entry ], [ %j.next, %outer.latch ]
  br label %inner.header

inner.header:
  %i = phi i32 [ 0, %outer.header ], [ %i.next, %inner.latch ]
  %i.lt = icmp slt i32 %i, %m
  br i1 %i.lt, label %inner.body, label %outer.latch

inner.body:
  br label %inner.latch

inner.latch:
  %i.next = add nsw i32 %i, 1
  br label %inner.header

outer.latch:
  %j.next = add nsw i32 %j, 1
  %j.lt = icmp slt i32 %j.next, %n
  br i1 %j.lt, label %outer.header, label %exit

exit:
  ret i32 0
}
