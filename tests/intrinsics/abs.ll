; ---
; description = "llvm.abs returns |x|; INT_MIN wraps or is poison per the flag"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; The i1 operand is LLVM's is_int_min_poison: `false` defines abs(INT_MIN) as
; INT_MIN (two's-complement wrap); `true` makes it poison, which must not raise a
; property failure when the result is guarded away.

declare i32 @llvm.abs.i32(i32, i1)
declare i8 @llvm.abs.i8(i8, i1)
declare i1 @llvm.abs.i1(i1, i1)
declare <2 x i16> @llvm.abs.v2i16(<2 x i16>, i1)
declare i32 @__CPROVER_nondet_int()
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %a = call i32 @llvm.abs.i32(i32 -5, i1 true)
  %c0 = icmp eq i32 %a, 5
  %b = call i32 @llvm.abs.i32(i32 7, i1 true)
  %c1 = icmp eq i32 %b, 7
  %m = call i8 @llvm.abs.i8(i8 -128, i1 false)
  %c2 = icmp eq i8 %m, -128
  %v = call <2 x i16> @llvm.abs.v2i16(<2 x i16> <i16 -3, i16 4>, i1 false)
  %v0 = extractelement <2 x i16> %v, i64 0
  %v1 = extractelement <2 x i16> %v, i64 1
  %c3 = icmp eq i16 %v0, 3
  %c4 = icmp eq i16 %v1, 4
  %x = call i32 @__CPROVER_nondet_int()
  %p = call i32 @llvm.abs.i32(i32 %x, i1 true)
  %d = call i32 @llvm.abs.i32(i32 %x, i1 false)
  %min = icmp eq i32 %x, -2147483648
  %g = select i1 %min, i32 -2147483648, i32 %p
  %c5 = icmp eq i32 %g, %d
  %xb = trunc i32 %x to i1
  %ab = call i1 @llvm.abs.i1(i1 %xb, i1 false)              ; 1-bit abs is the identity
  %c6 = icmp eq i1 %ab, %xb
  call void @__CPROVER_assert(i1 %c0, ptr null)
  call void @__CPROVER_assert(i1 %c1, ptr null)
  call void @__CPROVER_assert(i1 %c2, ptr null)
  call void @__CPROVER_assert(i1 %c3, ptr null)
  call void @__CPROVER_assert(i1 %c4, ptr null)
  call void @__CPROVER_assert(i1 %c5, ptr null)
  call void @__CPROVER_assert(i1 %c6, ptr null)
  ret i32 0
}
