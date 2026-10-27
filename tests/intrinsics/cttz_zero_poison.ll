; ---
; description = "cttz of zero with is_zero_poison yields an unconstrained (poison) result"
; expected = "VERIFICATION FAILED"
; entry = "main"
; ---
;
; Csmith seeds that call __builtin_ctz(0) are UB; the translation must not pin the
; poison result to the bit width and silently "prove" a checksum.

declare i32 @llvm.cttz.i32(i32, i1)
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %p = call i32 @llvm.cttz.i32(i32 0, i1 true)
  %ok = icmp eq i32 %p, 32
  call void @__CPROVER_assert(i1 %ok, ptr null)
  ret i32 0
}
