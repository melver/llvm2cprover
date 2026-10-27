; ---
; description = "ctpop/cttz/ctlz/bswap/bitreverse on integer vectors apply per lane"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; The SLP vectorizer emits vector bit intrinsics such as llvm.ctpop.v4i32 and
; llvm.cttz.v8i8 (Csmith --builtins seeds 1002, 1582, 1687, 2509).

declare <2 x i32> @llvm.ctpop.v2i32(<2 x i32>)
declare <2 x i8> @llvm.cttz.v2i8(<2 x i8>, i1)
declare <2 x i16> @llvm.ctlz.v2i16(<2 x i16>, i1)
declare <2 x i16> @llvm.bswap.v2i16(<2 x i16>)
declare <2 x i8> @llvm.bitreverse.v2i8(<2 x i8>)
declare <2 x i1> @llvm.ctpop.v2i1(<2 x i1>)
declare <2 x i1> @llvm.cttz.v2i1(<2 x i1>, i1)
declare <2 x i1> @llvm.ctlz.v2i1(<2 x i1>, i1)
declare <2 x i1> @llvm.bitreverse.v2i1(<2 x i1>)
declare i8 @__CPROVER_nondet_uchar()
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %pop = call <2 x i32> @llvm.ctpop.v2i32(<2 x i32> <i32 7, i32 -1>)
  %tz = call <2 x i8> @llvm.cttz.v2i8(<2 x i8> <i8 8, i8 0>, i1 false)
  %lz = call <2 x i16> @llvm.ctlz.v2i16(<2 x i16> <i16 1, i16 256>, i1 true)
  %bs = call <2 x i16> @llvm.bswap.v2i16(<2 x i16> <i16 258, i16 1>)
  %br = call <2 x i8> @llvm.bitreverse.v2i8(<2 x i8> <i8 1, i8 6>)
  %pop0 = extractelement <2 x i32> %pop, i64 0
  %pop1 = extractelement <2 x i32> %pop, i64 1
  %tz0 = extractelement <2 x i8> %tz, i64 0
  %tz1 = extractelement <2 x i8> %tz, i64 1
  %lz0 = extractelement <2 x i16> %lz, i64 0
  %lz1 = extractelement <2 x i16> %lz, i64 1
  %bs0 = extractelement <2 x i16> %bs, i64 0
  %bs1 = extractelement <2 x i16> %bs, i64 1
  %br0 = extractelement <2 x i8> %br, i64 0
  %br1 = extractelement <2 x i8> %br, i64 1
  %c0 = icmp eq i32 %pop0, 3
  %c1 = icmp eq i32 %pop1, 32
  %c2 = icmp eq i8 %tz0, 3
  %c3 = icmp eq i8 %tz1, 8
  %c4 = icmp eq i16 %lz0, 15
  %c5 = icmp eq i16 %lz1, 7
  %c6 = icmp eq i16 %bs0, 513
  %c7 = icmp eq i16 %bs1, 256
  %c8 = icmp eq i8 %br0, -128
  %c9 = icmp eq i8 %br1, 96
  call void @__CPROVER_assert(i1 %c0, ptr null)
  call void @__CPROVER_assert(i1 %c1, ptr null)
  call void @__CPROVER_assert(i1 %c2, ptr null)
  call void @__CPROVER_assert(i1 %c3, ptr null)
  call void @__CPROVER_assert(i1 %c4, ptr null)
  call void @__CPROVER_assert(i1 %c5, ptr null)
  call void @__CPROVER_assert(i1 %c6, ptr null)
  call void @__CPROVER_assert(i1 %c7, ptr null)
  call void @__CPROVER_assert(i1 %c8, ptr null)
  call void @__CPROVER_assert(i1 %c9, ptr null)

  ; <N x i1> lanes go through the 1-bit special cases per lane.
  %m = insertelement <2 x i1> <i1 false, i1 false>, i1 true, i64 1   ; <0, 1>
  %popm = call <2 x i1> @llvm.ctpop.v2i1(<2 x i1> %m)
  %tzm = call <2 x i1> @llvm.cttz.v2i1(<2 x i1> %m, i1 false)
  %lzm = call <2 x i1> @llvm.ctlz.v2i1(<2 x i1> %m, i1 false)
  %brm = call <2 x i1> @llvm.bitreverse.v2i1(<2 x i1> %m)
  %ok1 = icmp eq <2 x i1> %popm, %m
  %ok2 = icmp ne <2 x i1> %tzm, %m           ; cttz(0) = 1, cttz(1) = 0
  %ok3 = icmp ne <2 x i1> %lzm, %m
  %ok4 = icmp eq <2 x i1> %brm, %m
  %a1 = and <2 x i1> %ok1, %ok2
  %a2 = and <2 x i1> %a1, %ok3
  %a3 = and <2 x i1> %a2, %ok4
  %a30 = extractelement <2 x i1> %a3, i64 0
  %a31 = extractelement <2 x i1> %a3, i64 1
  call void @__CPROVER_assert(i1 %a30, ptr null)
  call void @__CPROVER_assert(i1 %a31, ptr null)

  ; A poisoned lane (zero with is_zero_poison) leaves the other lanes defined.
  %x = call i8 @__CPROVER_nondet_uchar()
  %xv = insertelement <2 x i8> <i8 0, i8 4>, i8 %x, i64 0
  %tzp = call <2 x i8> @llvm.cttz.v2i8(<2 x i8> %xv, i1 true)
  %tzp1 = extractelement <2 x i8> %tzp, i64 1
  %c10 = icmp eq i8 %tzp1, 2
  call void @__CPROVER_assert(i1 %c10, ptr null)
  ret i32 0
}
