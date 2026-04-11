; ---
; description = "fixed-width integer vector min/max intrinsics lower lane-wise"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare void @__CPROVER_assert(i1, ptr)

declare <4 x i8> @llvm.umin.v4i8(<4 x i8>, <4 x i8>)
declare <4 x i8> @llvm.umax.v4i8(<4 x i8>, <4 x i8>)
declare <4 x i8> @llvm.smin.v4i8(<4 x i8>, <4 x i8>)
declare <4 x i8> @llvm.smax.v4i8(<4 x i8>, <4 x i8>)

@.msg = private unnamed_addr constant [14 x i8] c"vector minmax\00"

define i32 @main() {
entry:
  %lhs = insertelement <4 x i8> poison, i8 -1, i64 0
  %lhs.1 = insertelement <4 x i8> %lhs, i8 1, i64 1
  %lhs.2 = insertelement <4 x i8> %lhs.1, i8 -128, i64 2
  %lhs.3 = insertelement <4 x i8> %lhs.2, i8 10, i64 3

  %rhs = insertelement <4 x i8> poison, i8 1, i64 0
  %rhs.1 = insertelement <4 x i8> %rhs, i8 2, i64 1
  %rhs.2 = insertelement <4 x i8> %rhs.1, i8 127, i64 2
  %rhs.3 = insertelement <4 x i8> %rhs.2, i8 10, i64 3

  %umin = call <4 x i8> @llvm.umin.v4i8(<4 x i8> %lhs.3, <4 x i8> %rhs.3)
  %umax = call <4 x i8> @llvm.umax.v4i8(<4 x i8> %lhs.3, <4 x i8> %rhs.3)
  %smin = call <4 x i8> @llvm.smin.v4i8(<4 x i8> %lhs.3, <4 x i8> %rhs.3)
  %smax = call <4 x i8> @llvm.smax.v4i8(<4 x i8> %lhs.3, <4 x i8> %rhs.3)

  %umin0 = extractelement <4 x i8> %umin, i64 0
  %umin1 = extractelement <4 x i8> %umin, i64 1
  %umin2 = extractelement <4 x i8> %umin, i64 2
  %umin3 = extractelement <4 x i8> %umin, i64 3
  %umax0 = extractelement <4 x i8> %umax, i64 0
  %umax1 = extractelement <4 x i8> %umax, i64 1
  %umax2 = extractelement <4 x i8> %umax, i64 2
  %umax3 = extractelement <4 x i8> %umax, i64 3
  %smin0 = extractelement <4 x i8> %smin, i64 0
  %smin1 = extractelement <4 x i8> %smin, i64 1
  %smin2 = extractelement <4 x i8> %smin, i64 2
  %smin3 = extractelement <4 x i8> %smin, i64 3
  %smax0 = extractelement <4 x i8> %smax, i64 0
  %smax1 = extractelement <4 x i8> %smax, i64 1
  %smax2 = extractelement <4 x i8> %smax, i64 2
  %smax3 = extractelement <4 x i8> %smax, i64 3

  %ok0 = icmp eq i8 %umin0, 1
  %ok1 = icmp eq i8 %umin1, 1
  %ok2 = icmp eq i8 %umin2, 127
  %ok3 = icmp eq i8 %umin3, 10
  %ok4 = icmp eq i8 %umax0, -1
  %ok5 = icmp eq i8 %umax1, 2
  %ok6 = icmp eq i8 %umax2, -128
  %ok7 = icmp eq i8 %umax3, 10
  %ok8 = icmp eq i8 %smin0, -1
  %ok9 = icmp eq i8 %smin1, 1
  %ok10 = icmp eq i8 %smin2, -128
  %ok11 = icmp eq i8 %smin3, 10
  %ok12 = icmp eq i8 %smax0, 1
  %ok13 = icmp eq i8 %smax1, 2
  %ok14 = icmp eq i8 %smax2, 127
  %ok15 = icmp eq i8 %smax3, 10

  %all0 = and i1 %ok0, %ok1
  %all1 = and i1 %all0, %ok2
  %all2 = and i1 %all1, %ok3
  %all3 = and i1 %all2, %ok4
  %all4 = and i1 %all3, %ok5
  %all5 = and i1 %all4, %ok6
  %all6 = and i1 %all5, %ok7
  %all7 = and i1 %all6, %ok8
  %all8 = and i1 %all7, %ok9
  %all9 = and i1 %all8, %ok10
  %all10 = and i1 %all9, %ok11
  %all11 = and i1 %all10, %ok12
  %all12 = and i1 %all11, %ok13
  %all13 = and i1 %all12, %ok14
  %all14 = and i1 %all13, %ok15
  call void @__CPROVER_assert(i1 %all14, ptr @.msg)
  ret i32 0
}
