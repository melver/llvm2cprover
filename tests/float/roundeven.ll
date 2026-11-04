; ---
; description = "llvm.roundeven rounds ties to even, unlike llvm.round which it used to be lowered as"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare double @llvm.roundeven.f64(double)
declare double @llvm.round.f64(double)
declare <2 x float> @llvm.roundeven.v2f32(<2 x float>)
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %e = call double @llvm.roundeven.f64(double 2.5)
  %ok0 = fcmp oeq double %e, 2.0
  call void @__CPROVER_assert(i1 %ok0, ptr null)
  %r = call double @llvm.round.f64(double 2.5)
  %ok1 = fcmp oeq double %r, 3.0
  call void @__CPROVER_assert(i1 %ok1, ptr null)
  %v = call <2 x float> @llvm.roundeven.v2f32(<2 x float> <float -3.5, float 0.5>)
  %v0 = extractelement <2 x float> %v, i64 0
  %v1 = extractelement <2 x float> %v, i64 1
  %ok2 = fcmp oeq float %v0, -4.0
  %ok3 = fcmp oeq float %v1, 0.0
  %ok23 = and i1 %ok2, %ok3
  call void @__CPROVER_assert(i1 %ok23, ptr null)
  ret i32 0
}
