; ---
; description = "llvm.powi lowers to CBMC's __builtin_powi(f) with a signed exponent of any width"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; esbmc_skip = "ESBMC has no __builtin_powi models for CBMC binaries"
; ---
;
; CBMC's powi model is exact only on its special cases, so only those are asserted. Zero to a
; negative power is +/-inf; zero-extending the i16 exponent -1 to 65535 would give +0.0 instead.

declare double @llvm.powi.f64.i32(double, i32)
declare double @llvm.powi.f64.i16(double, i16)
declare float @llvm.powi.f32.i32(float, i32)
declare <2 x float> @llvm.powi.v2f32.i32(<2 x float>, i32)
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %a = call double @llvm.powi.f64.i32(double 7.5, i32 0)
  %ok0 = fcmp oeq double %a, 1.0
  call void @__CPROVER_assert(i1 %ok0, ptr null)
  %b = call double @llvm.powi.f64.i16(double 0.0, i16 -1)
  %ok1 = fcmp oeq double %b, 0x7FF0000000000000
  call void @__CPROVER_assert(i1 %ok1, ptr null)
  %c = call float @llvm.powi.f32.i32(float -0.0, i32 -3)
  %ok2 = fcmp oeq float %c, 0xFFF0000000000000
  call void @__CPROVER_assert(i1 %ok2, ptr null)
  %v = call <2 x float> @llvm.powi.v2f32.i32(<2 x float> <float 0.0, float 1.0>, i32 -2)
  %v0 = extractelement <2 x float> %v, i64 0
  %v1 = extractelement <2 x float> %v, i64 1
  %ok3 = fcmp oeq float %v0, 0x7FF0000000000000
  %ok4 = fcmp oeq float %v1, 1.0
  %ok34 = and i1 %ok3, %ok4
  call void @__CPROVER_assert(i1 %ok34, ptr null)
  ret i32 0
}
