; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
; ---
; description = "Common floating-point LLVM intrinsics map to CBMC builtins"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare float @llvm.fabs.f32(float)
declare float @llvm.sqrt.f32(float)
declare float @llvm.fma.f32(float, float, float)
declare double @llvm.rint.f64(double)
declare double @llvm.nearbyint.f64(double)
declare void @__CPROVER_assert(i1, ptr)

@.msg1 = private unnamed_addr constant [5 x i8] c"fabs\00"
@.msg2 = private unnamed_addr constant [5 x i8] c"sqrt\00"
@.msg3 = private unnamed_addr constant [4 x i8] c"fma\00"
@.msg4 = private unnamed_addr constant [5 x i8] c"rint\00"
@.msg5 = private unnamed_addr constant [10 x i8] c"nearbyint\00"

define i32 @main() {
entry:
  %a = call float @llvm.fabs.f32(float -4.000000e+00)
  %b = call float @llvm.sqrt.f32(float 9.000000e+00)
  %c = call float @llvm.fma.f32(float 2.000000e+00, float 3.000000e+00, float 4.000000e+00)
  %d = call double @llvm.rint.f64(double 2.500000e+00)
  %e = call double @llvm.nearbyint.f64(double 2.500000e+00)
  %ok1 = fcmp oeq float %a, 4.000000e+00
  %ok2 = fcmp oeq float %b, 3.000000e+00
  %ok3 = fcmp oeq float %c, 1.000000e+01
  %ok4 = fcmp oeq double %d, 2.000000e+00
  %ok5 = fcmp oeq double %e, 2.000000e+00
  call void @__CPROVER_assert(i1 %ok1, ptr @.msg1)
  call void @__CPROVER_assert(i1 %ok2, ptr @.msg2)
  call void @__CPROVER_assert(i1 %ok3, ptr @.msg3)
  call void @__CPROVER_assert(i1 %ok4, ptr @.msg4)
  call void @__CPROVER_assert(i1 %ok5, ptr @.msg5)
  ret i32 0
}
