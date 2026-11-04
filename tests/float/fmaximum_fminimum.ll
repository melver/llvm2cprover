; ---
; description = "llvm.maximum/minimum propagate NaN and order -0 below +0, unlike maxnum/minnum (fmax/fmin)"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare double @llvm.maximum.f64(double, double)
declare double @llvm.minimum.f64(double, double)
declare <2 x float> @llvm.maximum.v2f32(<2 x float>, <2 x float>)
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  ; NaN in either position propagates.
  %n0 = call double @llvm.maximum.f64(double 0x7FF8000000000000, double 1.0)
  %ok0 = fcmp uno double %n0, 0.0
  call void @__CPROVER_assert(i1 %ok0, ptr null)
  %n1 = call double @llvm.minimum.f64(double 1.0, double 0x7FF8000000000000)
  %ok1 = fcmp uno double %n1, 0.0
  call void @__CPROVER_assert(i1 %ok1, ptr null)

  ; -0 < +0, in both operand orders: check the sign via bitcast.
  %z0 = call double @llvm.maximum.f64(double -0.0, double 0.0)
  %z1 = call double @llvm.maximum.f64(double 0.0, double -0.0)
  %z2 = call double @llvm.minimum.f64(double -0.0, double 0.0)
  %z3 = call double @llvm.minimum.f64(double 0.0, double -0.0)
  %b0 = bitcast double %z0 to i64
  %b1 = bitcast double %z1 to i64
  %b2 = bitcast double %z2 to i64
  %b3 = bitcast double %z3 to i64
  %ok2 = icmp eq i64 %b0, 0
  %ok3 = icmp eq i64 %b1, 0
  %ok4 = icmp eq i64 %b2, -9223372036854775808
  %ok5 = icmp eq i64 %b3, -9223372036854775808
  call void @__CPROVER_assert(i1 %ok2, ptr null)
  call void @__CPROVER_assert(i1 %ok3, ptr null)
  call void @__CPROVER_assert(i1 %ok4, ptr null)
  call void @__CPROVER_assert(i1 %ok5, ptr null)

  ; Ordinary values, lane-wise.
  %v = call <2 x float> @llvm.maximum.v2f32(<2 x float> <float 1.0, float -2.0>, <2 x float> <float 3.0, float -5.0>)
  %v0 = extractelement <2 x float> %v, i64 0
  %v1 = extractelement <2 x float> %v, i64 1
  %ok6 = fcmp oeq float %v0, 3.0
  %ok7 = fcmp oeq float %v1, -2.0
  %ok67 = and i1 %ok6, %ok7
  call void @__CPROVER_assert(i1 %ok67, ptr null)
  ret i32 0
}
