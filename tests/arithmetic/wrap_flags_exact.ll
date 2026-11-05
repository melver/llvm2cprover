; ---
; description = "nsw/nuw arithmetic is exact when it does not wrap; discarded poison does not matter"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; esbmc_skip = "ESBMC --overflow-check instruments the signed operation inside CBMC's overflow-* predicate"
; ---
;
; Overflowing nsw/nuw results are poison (nondet, see wrap_flags_poison.ll); everything else must
; stay exact, including flag boundaries and unflagged wrapping. The select mirrors clang -O2,
; which hoists `mul nsw` above the guard that discards an overflowing result.

declare i32 @__CPROVER_nondet_int()
declare void @__CPROVER_assume(i1)
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %a = add nsw i8 127, 0
  %ok0 = icmp eq i8 %a, 127
  call void @__CPROVER_assert(i1 %ok0, ptr null)
  %m = mul nsw i8 -128, 1
  %ok1 = icmp eq i8 %m, -128
  call void @__CPROVER_assert(i1 %ok1, ptr null)
  %u = add nuw i8 200, 55
  %ok2 = icmp eq i8 %u, -1
  call void @__CPROVER_assert(i1 %ok2, ptr null)
  %s = sub nuw nsw i8 0, 0
  %ok3 = icmp eq i8 %s, 0
  call void @__CPROVER_assert(i1 %ok3, ptr null)
  %sh = shl nsw i8 -64, 1
  %ok4 = icmp eq i8 %sh, -128
  call void @__CPROVER_assert(i1 %ok4, ptr null)
  %shu = shl nuw i8 64, 1
  %ok5 = icmp eq i8 %shu, -128
  call void @__CPROVER_assert(i1 %ok5, ptr null)
  %wrap = add i8 127, 1
  %ok6 = icmp eq i8 %wrap, -128
  call void @__CPROVER_assert(i1 %ok6, ptr null)
  %v = add nsw <2 x i8> <i8 100, i8 -100>, <i8 27, i8 -28>
  %v0 = extractelement <2 x i8> %v, i64 0
  %v1 = extractelement <2 x i8> %v, i64 1
  %ok7 = icmp eq i8 %v0, 127
  %ok8 = icmp eq i8 %v1, -128
  %ok78 = and i1 %ok7, %ok8
  call void @__CPROVER_assert(i1 %ok78, ptr null)

  %x = call i32 @__CPROVER_nondet_int()
  %big = icmp sgt i32 %x, 65536
  call void @__CPROVER_assume(i1 %big)
  %sq = mul nsw i32 %x, %x
  %guard = icmp slt i32 %x, 46341
  %r = select i1 %guard, i32 %sq, i32 0
  %ok9 = icmp eq i32 %r, 0
  call void @__CPROVER_assert(i1 %ok9, ptr null)
  ret i32 0
}
