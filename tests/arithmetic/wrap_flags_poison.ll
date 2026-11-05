; ---
; description = "Overflowing nsw/nuw arithmetic is poison (nondet), not the wrapped result"
; expected = "VERIFICATION FAILED"
; entry = "main"
; ---
;
; Two identical overflowing `mul nsw` need not agree. Previously both wrapped to the same value,
; so proofs silently assumed -fwrapv semantics for C signed arithmetic.

declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %a = mul nsw i32 600376, 4096
  %b = mul nsw i32 600376, 4096
  %same = icmp eq i32 %a, %b
  call void @__CPROVER_assert(i1 %same, ptr null)
  ret i32 0
}
