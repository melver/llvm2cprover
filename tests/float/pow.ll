; ---
; description = "llvm.pow lowers to CBMC's pow/powf, scalar and per vector lane"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; esbmc_skip = "ESBMC has no pow models for CBMC binaries"
; ---
;
; CBMC's pow model is exact only on its special cases; elsewhere it over-approximates with a
; nondet bit-trick estimate (e.g. pow(2, 10) is only bounded within 2x), so only exact cases are
; asserted here.

declare double @llvm.pow.f64(double, double)
declare float @llvm.pow.f32(float, float)
declare <2 x float> @llvm.pow.v2f32(<2 x float>, <2 x float>)
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %d = call double @llvm.pow.f64(double 7.5, double 0.0)
  %ok0 = fcmp oeq double %d, 1.0
  call void @__CPROVER_assert(i1 %ok0, ptr null)
  %f = call float @llvm.pow.f32(float 1.0, float 3.25)
  %ok1 = fcmp oeq float %f, 1.0
  call void @__CPROVER_assert(i1 %ok1, ptr null)
  %nan = call double @llvm.pow.f64(double -2.0, double 0.5)
  %ok2 = fcmp uno double %nan, 0.0
  call void @__CPROVER_assert(i1 %ok2, ptr null)
  %v = call <2 x float> @llvm.pow.v2f32(<2 x float> <float 9.0, float 1.0>, <2 x float> <float 0.0, float 5.0>)
  %v0 = extractelement <2 x float> %v, i64 0
  %v1 = extractelement <2 x float> %v, i64 1
  %ok3 = fcmp oeq float %v0, 1.0
  %ok4 = fcmp oeq float %v1, 1.0
  %ok34 = and i1 %ok3, %ok4
  call void @__CPROVER_assert(i1 %ok34, ptr null)
  ret i32 0
}
