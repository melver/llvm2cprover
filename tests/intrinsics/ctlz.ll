; ---
; description = "llvm.ctlz counts leading zeros; zero input is the bit width or poison per the flag"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; Csmith --builtins emits __builtin_clz/clzl/clzll as llvm.ctlz.i32/i64.

declare i1 @llvm.ctlz.i1(i1, i1)
declare i7 @llvm.ctlz.i7(i7, i1)
declare i32 @llvm.ctlz.i32(i32, i1)
declare i64 @llvm.ctlz.i64(i64, i1)
declare i128 @llvm.ctlz.i128(i128, i1)
declare i32 @__CPROVER_nondet_int()
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %a = call i32 @llvm.ctlz.i32(i32 1, i1 true)
  %c0 = icmp eq i32 %a, 31
  %b = call i64 @llvm.ctlz.i64(i64 4294967296, i1 true)      ; 1 << 32
  %c1 = icmp eq i64 %b, 31
  %z = call i32 @llvm.ctlz.i32(i32 0, i1 false)
  %c2 = icmp eq i32 %z, 32
  %o = call i1 @llvm.ctlz.i1(i1 true, i1 false)
  %c3 = icmp eq i1 %o, false
  %x = call i32 @__CPROVER_nondet_int()
  %p = call i32 @llvm.ctlz.i32(i32 %x, i1 true)
  %d = call i32 @llvm.ctlz.i32(i32 %x, i1 false)
  %x0 = icmp eq i32 %x, 0
  %g = select i1 %x0, i32 32, i32 %p
  %c4 = icmp eq i32 %g, %d
  %z7 = call i7 @llvm.ctlz.i7(i7 0, i1 false)
  %c5 = icmp eq i7 %z7, 7
  %z128 = call i128 @llvm.ctlz.i128(i128 0, i1 false)
  %c6 = icmp eq i128 %z128, 128
  call void @__CPROVER_assert(i1 %c0, ptr null)
  call void @__CPROVER_assert(i1 %c1, ptr null)
  call void @__CPROVER_assert(i1 %c2, ptr null)
  call void @__CPROVER_assert(i1 %c3, ptr null)
  call void @__CPROVER_assert(i1 %c4, ptr null)
  call void @__CPROVER_assert(i1 %c5, ptr null)
  call void @__CPROVER_assert(i1 %c6, ptr null)
  ret i32 0
}
