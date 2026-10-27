; ---
; description = "cttz with a defined zero input returns the bit width without a zero check"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; The i1 flag is LLVM's is_zero_poison: `false` defines cttz(0) as the bit width.
; Clang -O2 also guards `cttz(x, true)` with a select on `x == 0`, so a poison
; result that is never used must not raise a property failure either.

declare i32 @llvm.cttz.i32(i32, i1)
declare i7 @llvm.cttz.i7(i7, i1)
declare i128 @llvm.cttz.i128(i128, i1)
declare i32 @__CPROVER_nondet_int()
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %x = call i32 @__CPROVER_nondet_int()
  %d = call i32 @llvm.cttz.i32(i32 %x, i1 false)
  %p = call i32 @llvm.cttz.i32(i32 %x, i1 true)
  %z = icmp eq i32 %x, 0
  %guarded = select i1 %z, i32 32, i32 %p
  %ok0 = icmp eq i32 %d, %guarded
  %d0 = call i32 @llvm.cttz.i32(i32 0, i1 false)
  %ok1 = icmp eq i32 %d0, 32
  ; The zero result is the operand width, also for odd and wide integers.
  %d7 = call i7 @llvm.cttz.i7(i7 0, i1 false)
  %ok2 = icmp eq i7 %d7, 7
  %d128 = call i128 @llvm.cttz.i128(i128 0, i1 false)
  %ok3 = icmp eq i128 %d128, 128
  call void @__CPROVER_assert(i1 %ok0, ptr null)
  call void @__CPROVER_assert(i1 %ok1, ptr null)
  call void @__CPROVER_assert(i1 %ok2, ptr null)
  call void @__CPROVER_assert(i1 %ok3, ptr null)
  ret i32 0
}
