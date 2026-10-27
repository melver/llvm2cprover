; ---
; description = "llvm.bitreverse reverses the bit order of iN values"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; Csmith seed 813 (-O2) emits `llvm.bitreverse.i1`.

declare i1 @llvm.bitreverse.i1(i1)
declare i3 @llvm.bitreverse.i3(i3)
declare i8 @llvm.bitreverse.i8(i8)
declare i32 @llvm.bitreverse.i32(i32)
declare i128 @llvm.bitreverse.i128(i128)
declare i8 @__CPROVER_nondet_uchar()
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %x = call i8 @__CPROVER_nondet_uchar()
  %b = trunc i8 %x to i1
  %r1 = call i1 @llvm.bitreverse.i1(i1 %b)
  %c0 = icmp eq i1 %r1, %b
  %r8 = call i8 @llvm.bitreverse.i8(i8 6)          ; 0b00000110 -> 0b01100000
  %c1 = icmp eq i8 %r8, 96
  %r32 = call i32 @llvm.bitreverse.i32(i32 1)
  %c2 = icmp eq i32 %r32, -2147483648
  %rr = call i8 @llvm.bitreverse.i8(i8 %x)
  %rrr = call i8 @llvm.bitreverse.i8(i8 %rr)
  %c3 = icmp eq i8 %rrr, %x
  %r3 = call i3 @llvm.bitreverse.i3(i3 1)          ; 0b001 -> 0b100
  %c4 = icmp eq i3 %r3, -4
  %r128 = call i128 @llvm.bitreverse.i128(i128 1)
  %c5 = icmp eq i128 %r128, -170141183460469231731687303715884105728
  call void @__CPROVER_assert(i1 %c0, ptr null)
  call void @__CPROVER_assert(i1 %c1, ptr null)
  call void @__CPROVER_assert(i1 %c2, ptr null)
  call void @__CPROVER_assert(i1 %c3, ptr null)
  call void @__CPROVER_assert(i1 %c4, ptr null)
  call void @__CPROVER_assert(i1 %c5, ptr null)
  ret i32 0
}
