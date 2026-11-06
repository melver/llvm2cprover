; ---
; description = "An inner loop entered by a forward jump from the outer header has its unwinding counted per entry"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; cbmc_flags = ["--unwind", "4", "--unwinding-assertions"]
; todo = "known CBMC bug (6.11.0): symex treats an unconditional forward goto as a back edge and does not reset the target loop's unwinding counter"
; ---

; Outer loop runs 2 times, inner loop 3 times per outer iteration. The outer
; header's conditional branch makes the GOTO program enter the inner loop head
; by an unconditional forward GOTO rather than by fallthrough; CBMC then counts
; inner iterations across outer iterations and f.unwind.0 fails at --unwind 4.
; With a plain `br label %inner` the same loops verify.

define i32 @f() {
start:
  %i = alloca i32
  %j = alloca i32
  store i32 0, ptr %i
  br label %outer

outer:
  store i32 0, ptr %j
  br i1 true, label %inner, label %early

early:
  ret i32 1

inner:
  %jv = load i32, ptr %j
  %jn = add i32 %jv, 1
  store i32 %jn, ptr %j
  %more = icmp slt i32 %jn, 3
  br i1 %more, label %inner, label %latch

latch:
  %iv = load i32, ptr %i
  %in = add i32 %iv, 1
  store i32 %in, ptr %i
  %again = icmp slt i32 %in, 2
  br i1 %again, label %outer, label %done

done:
  ret i32 0
}

define i32 @main() {
  %r = call i32 @f()
  ret i32 %r
}
