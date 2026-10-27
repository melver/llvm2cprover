; ---
; description = "a zero lane of vector cttz with is_zero_poison yields an unconstrained result"
; expected = "VERIFICATION FAILED"
; entry = "main"
; ---
;
; Companion to vector_bit_intrinsics.ll: the poisoned lane must not be pinned to the
; bit width.

declare <2 x i8> @llvm.cttz.v2i8(<2 x i8>, i1)
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %r = call <2 x i8> @llvm.cttz.v2i8(<2 x i8> <i8 0, i8 4>, i1 true)
  %r0 = extractelement <2 x i8> %r, i64 0
  %ok = icmp eq i8 %r0, 8
  call void @__CPROVER_assert(i1 %ok, ptr null)
  ret i32 0
}
