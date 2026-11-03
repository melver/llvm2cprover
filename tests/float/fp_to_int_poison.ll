; ---
; description = "fptosi of an out-of-range value is poison (nondet), not CBMC's deterministic cast result"
; expected = "VERIFICATION FAILED"
; entry = "main"
; ---
;
; Two conversions of the same out-of-range value need not agree; previously both lowered to the
; same CBMC typecast, so this assertion held. Found via Csmith --float seeds 88/182/247/665/695.

declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %a = fptosi double 2147483648.0 to i32
  %b = fptosi double 2147483648.0 to i32
  %same = icmp eq i32 %a, %b
  call void @__CPROVER_assert(i1 %same, ptr null)
  ret i32 0
}
