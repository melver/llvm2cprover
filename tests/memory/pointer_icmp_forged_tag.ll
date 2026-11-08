; ---
; description = "Integer addresses with CBMC object bits set forge object tags, so relational icmp is unreliable"
; expected = "VERIFICATION FAILED"
; entry = "main"
; esbmc_skip = "documents a CBMC pointer-encoding limitation"
; ---
;
; Known limitation, not a goal. CBMC encodes a pointer as an object number in
; the top `--object-bits` bits and a signed offset in the rest; `inttoptr` just
; reinterprets the integer. Addresses at or above 2^(63 - object_bits), e.g.
; kernel addresses, therefore forge object tags: CBMC's memory layout does not
; match the target's, and equality, dereference, and bounds checks on such
; pointers are already unfaithful. Symex may also fold `same_object` of two
; integer-derived pointers to true even when their tags differ, so the in-bounds
; offset comparison applies across tags and `icmp ult ptr` may disagree with
; `icmp ult i64`.

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"

declare void @__CPROVER_assert(i1, ptr)
declare i64 @__CPROVER_nondet_u64()

define i32 @main() {
entry:
  %x = call i64 @__CPROVER_nondet_u64()
  %y = call i64 @__CPROVER_nondet_u64()
  %px = inttoptr i64 %x to ptr
  %py = inttoptr i64 %y to ptr
  %p_lt = icmp ult ptr %px, %py
  %i_lt = icmp ult i64 %x, %y
  %agree = icmp eq i1 %p_lt, %i_lt
  call void @__CPROVER_assert(i1 %agree, ptr null)
  ret i32 0
}
