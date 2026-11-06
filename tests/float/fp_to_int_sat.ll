; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
; ---
; description = "llvm.fptoui.sat / llvm.fptosi.sat saturate: NaN -> 0, out-of-range -> min/max"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare void @__CPROVER_assert(i1, ptr)
declare double @nondet_double()
declare i16 @llvm.fptoui.sat.i16.f64(double)
declare i32 @llvm.fptoui.sat.i32.f64(double)
declare i32 @llvm.fptosi.sat.i32.f64(double)
declare i1 @llvm.fptosi.sat.i1.f64(double)
declare <2 x i8> @llvm.fptosi.sat.v2i8.v2f32(<2 x float>)

@.m1 = private unnamed_addr constant [8 x i8] c"u16 nan\00"
@.m2 = private unnamed_addr constant [8 x i8] c"u16 neg\00"
@.m3 = private unnamed_addr constant [8 x i8] c"u16 big\00"
@.m4 = private unnamed_addr constant [9 x i8] c"u32 1.75\00"
@.m5 = private unnamed_addr constant [8 x i8] c"i32 low\00"
@.m6 = private unnamed_addr constant [11 x i8] c"u16 in rng\00"
@.m7 = private unnamed_addr constant [11 x i8] c"v2i8 clamp\00"
@.m8 = private unnamed_addr constant [7 x i8] c"i1 low\00"

define i32 @main() {
entry:
  %nan = fdiv double 0.000000e+00, 0.000000e+00
  %a = call i16 @llvm.fptoui.sat.i16.f64(double %nan)
  %c1 = icmp eq i16 %a, 0
  call void @__CPROVER_assert(i1 %c1, ptr @.m1)
  %b = call i16 @llvm.fptoui.sat.i16.f64(double -3.500000e+00)
  %c2 = icmp eq i16 %b, 0
  call void @__CPROVER_assert(i1 %c2, ptr @.m2)
  %c = call i16 @llvm.fptoui.sat.i16.f64(double 1.000000e+06)
  %c3 = icmp eq i16 %c, 65535
  call void @__CPROVER_assert(i1 %c3, ptr @.m3)
  %d = call i32 @llvm.fptoui.sat.i32.f64(double 1.750000e+00)
  %c4 = icmp eq i32 %d, 1
  call void @__CPROVER_assert(i1 %c4, ptr @.m4)
  %e = call i32 @llvm.fptosi.sat.i32.f64(double -1.000000e+20)
  %c5 = icmp eq i32 %e, -2147483648
  call void @__CPROVER_assert(i1 %c5, ptr @.m5)
  %v = call <2 x i8> @llvm.fptosi.sat.v2i8.v2f32(<2 x float> <float 3.000000e+02, float -3.000000e+02>)
  %vc = icmp eq <2 x i8> %v, <i8 127, i8 -128>
  %vb = bitcast <2 x i1> %vc to i2
  %c7 = icmp eq i2 %vb, -1
  call void @__CPROVER_assert(i1 %c7, ptr @.m7)
  %w = call i1 @llvm.fptosi.sat.i1.f64(double -5.000000e+00)
  call void @__CPROVER_assert(i1 %w, ptr @.m8)
  ; symbolic: an in-range input truncates toward zero
  %x = call double @nondet_double()
  %ge = fcmp oge double %x, 0.000000e+00
  %lt = fcmp olt double %x, 6.553500e+04
  %in = and i1 %ge, %lt
  br i1 %in, label %chk, label %done
chk:
  %s = call i16 @llvm.fptoui.sat.i16.f64(double %x)
  %t = fptoui double %x to i16
  %c6 = icmp eq i16 %s, %t
  call void @__CPROVER_assert(i1 %c6, ptr @.m6)
  br label %done
done:
  ret i32 0
}
