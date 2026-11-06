; ---
; description = "llvm.exp/exp2/log/log2/log10 lower to CBMC's math models, scalar and per vector lane"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; esbmc_skip = "ESBMC has no exp/log models for CBMC binaries"
; ---
;
; CBMC's models are exact only on their special cases (elsewhere they over-approximate), so only
; those are asserted here.

declare double @llvm.exp.f64(double)
declare float @llvm.exp2.f32(float)
declare double @llvm.log.f64(double)
declare float @llvm.log2.f32(float)
declare double @llvm.log10.f64(double)
declare <2 x double> @llvm.log.v2f64(<2 x double>)
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %e = call double @llvm.exp.f64(double 0xFFF0000000000000)
  %ok0 = fcmp oeq double %e, 0.0
  call void @__CPROVER_assert(i1 %ok0, ptr null)
  %e2 = call float @llvm.exp2.f32(float 0.0)
  %ok1 = fcmp oeq float %e2, 1.0
  call void @__CPROVER_assert(i1 %ok1, ptr null)
  %l = call double @llvm.log.f64(double 1.0)
  %ok2 = fcmp oeq double %l, 0.0
  call void @__CPROVER_assert(i1 %ok2, ptr null)
  %l2 = call float @llvm.log2.f32(float 1.0)
  %ok3 = fcmp oeq float %l2, 0.0
  call void @__CPROVER_assert(i1 %ok3, ptr null)
  %l10 = call double @llvm.log10.f64(double 1.0)
  %ok4 = fcmp oeq double %l10, 0.0
  call void @__CPROVER_assert(i1 %ok4, ptr null)
  %v = call <2 x double> @llvm.log.v2f64(<2 x double> <double 1.0, double -1.0>)
  %v0 = extractelement <2 x double> %v, i64 0
  %v1 = extractelement <2 x double> %v, i64 1
  %ok5 = fcmp oeq double %v0, 0.0
  %ok6 = fcmp uno double %v1, 0.0
  %ok56 = and i1 %ok5, %ok6
  call void @__CPROVER_assert(i1 %ok56, ptr null)
  ret i32 0
}
