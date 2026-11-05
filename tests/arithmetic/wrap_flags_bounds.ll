; ---
; description = "nsw/nuw checks dropped by range analysis are only on operations that cannot wrap"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; esbmc_skip = "ESBMC --overflow-check instruments the signed operation inside CBMC's overflow-* predicate"
; ---
;
; Each flagged i32 result, whose poison check the analysis removes, must equal the exact result
; computed in i64: had the analysis dropped a check on an operation that can wrap, the wrapped
; value would differ. Covers a packed `u | v << 16`, clamps, ashr, udiv/urem, a loop phi that is
; re-loaded each iteration, and a 16-term unrolled byte sum.

declare i8 @__CPROVER_nondet_uchar()
declare i32 @__CPROVER_nondet_int()
declare void @__CPROVER_assert(i1, ptr)
declare i32 @llvm.smin.i32(i32, i32)
declare i32 @llvm.smax.i32(i32, i32)

define void @exact(i32 %flagged, i64 %wide) {
  %ext = sext i32 %flagged to i64
  %ok = icmp eq i64 %ext, %wide
  call void @__CPROVER_assert(i1 %ok, ptr null)
  ret void
}

define i32 @main() {
entry:
  %a = call i8 @__CPROVER_nondet_uchar()
  %b = call i8 @__CPROVER_nondet_uchar()
  %p = call i32 @__CPROVER_nondet_int()
  %u = zext i8 %a to i32
  %v = zext i8 %b to i32
  %u64 = zext i8 %a to i64
  %v64 = zext i8 %b to i64
  %p64 = sext i32 %p to i64

  ; packed (v << 16 | u) * 3 + 0x20002
  %vs = shl nuw nsw i32 %v, 16
  %uv = or disjoint i32 %vs, %u
  %m3 = mul nuw nsw i32 %uv, 3
  %r0 = add nuw nsw i32 %m3, 131074
  %vs64 = shl i64 %v64, 16
  %uv64 = or i64 %vs64, %u64
  %m364 = mul i64 %uv64, 3
  %w0 = add i64 %m364, 131074
  call void @exact(i32 %r0, i64 %w0)

  ; clip to [0, 255], squared
  %lo = call i32 @llvm.smin.i32(i32 %p, i32 255)
  %c = call i32 @llvm.smax.i32(i32 %lo, i32 0)
  %r1 = mul nuw nsw i32 %c, %c
  %c64 = sext i32 %c to i64
  %w1 = mul i64 %c64, %c64
  call void @exact(i32 %r1, i64 %w1)

  ; (p >> 24) * 2, signed
  %h = ashr i32 %p, 24
  %r2 = add nsw i32 %h, %h
  %h64 = ashr i64 %p64, 24
  %w2 = add i64 %h64, %h64
  call void @exact(i32 %r2, i64 %w2)

  ; (p urem 100)^2 + (p udiv 2^16); the reference widens the i32 quotient and remainder, since
  ; proving i32 and i64 dividers equal is slow for SAT and they are not what is checked.
  %rem = urem i32 %p, 100
  %sq = mul nuw nsw i32 %rem, %rem
  %div = udiv i32 %p, 65536
  %r3 = add nuw nsw i32 %sq, %div
  %rem64 = zext i32 %rem to i64
  %sq64 = mul i64 %rem64, %rem64
  %div64 = zext i32 %div to i64
  %w3 = add i64 %sq64, %div64
  call void @exact(i32 %r3, i64 %w3)

  ; 8 + sixteen bytes
  %s1 = add nuw nsw i32 %u, 8
  %s2 = add nuw nsw i32 %s1, %v
  %s3 = add nuw nsw i32 %s2, %u
  %s4 = add nuw nsw i32 %s3, %v
  %s5 = add nuw nsw i32 %s4, %u
  %s6 = add nuw nsw i32 %s5, %v
  %s7 = add nuw nsw i32 %s6, %u
  %s8 = add nuw nsw i32 %s7, %v
  %s9 = add nuw nsw i32 %s8, %u
  %s10 = add nuw nsw i32 %s9, %v
  %s11 = add nuw nsw i32 %s10, %u
  %s12 = add nuw nsw i32 %s11, %v
  %s13 = add nuw nsw i32 %s12, %u
  %s14 = add nuw nsw i32 %s13, %v
  %s15 = add nuw nsw i32 %s14, %u
  %s16 = add nuw nsw i32 %s15, %v
  %sum8 = mul i64 %u64, 8
  %sumv = mul i64 %v64, 8
  %sum = add i64 %sum8, %sumv
  %w4 = add i64 %sum, 8
  call void @exact(i32 %s16, i64 %w4)
  br label %loop

  ; a loop phi re-loaded each iteration: acc * 3 + acc
loop:
  %i = phi i32 [ 0, %entry ], [ %i.next, %loop ]
  %acc = phi i32 [ %u, %entry ], [ %next, %loop ]
  %t = mul nuw nsw i32 %acc, 3
  %r5 = add nuw nsw i32 %t, %acc
  %acc64 = zext i32 %acc to i64
  %w5 = mul i64 %acc64, 4
  call void @exact(i32 %r5, i64 %w5)
  %n = call i8 @__CPROVER_nondet_uchar()
  %next = zext i8 %n to i32
  %i.next = add nuw nsw i32 %i, 1
  %done = icmp eq i32 %i.next, 3
  br i1 %done, label %exit, label %loop

exit:
  ret i32 0
}
