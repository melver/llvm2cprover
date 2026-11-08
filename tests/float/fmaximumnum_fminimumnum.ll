; ---
; description = "llvm.maximumnum/minimumnum return the non-NaN operand and order -0 below +0"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare double @llvm.maximumnum.f64(double, double)
declare double @llvm.minimumnum.f64(double, double)
declare float @llvm.maximumnum.f32(float, float)
declare float @llvm.minimumnum.f32(float, float)
declare <2 x float> @llvm.maximumnum.v2f32(<2 x float>, <2 x float>)
declare float @__CPROVER_nondet_float()
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  ; A NaN in either position yields the other (symbolic) operand; two NaNs yield NaN.
  %x = call float @__CPROVER_nondet_float()
  %mx = call float @llvm.maximumnum.f32(float %x, float 0x7FF8000000000000)
  %mn = call float @llvm.minimumnum.f32(float 0x7FF8000000000000, float %x)
  %x_nan = fcmp uno float %x, %x
  %mx_ok = fcmp oeq float %mx, %x
  %mn_ok = fcmp oeq float %mn, %x
  %both = and i1 %mx_ok, %mn_ok
  %mx_nan = fcmp uno float %mx, 0.0
  %mn_nan = fcmp uno float %mn, 0.0
  %both_nan = and i1 %mx_nan, %mn_nan
  %nan_ok = select i1 %x_nan, i1 %both_nan, i1 %both
  call void @__CPROVER_assert(i1 %nan_ok, ptr null)

  ; -0 < +0, in both operand orders: check the sign via bitcast.
  %z0 = call double @llvm.maximumnum.f64(double -0.0, double 0.0)
  %z1 = call double @llvm.minimumnum.f64(double 0.0, double -0.0)
  %b0 = bitcast double %z0 to i64
  %b1 = bitcast double %z1 to i64
  %ok3 = icmp eq i64 %b0, 0
  %ok4 = icmp eq i64 %b1, -9223372036854775808
  call void @__CPROVER_assert(i1 %ok3, ptr null)
  call void @__CPROVER_assert(i1 %ok4, ptr null)

  ; Ordinary values and a NaN lane, lane-wise.
  %v = call <2 x float> @llvm.maximumnum.v2f32(<2 x float> <float 1.0, float 0x7FF8000000000000>, <2 x float> <float 2.0, float 3.0>)
  %v0 = extractelement <2 x float> %v, i32 0
  %v1 = extractelement <2 x float> %v, i32 1
  %ok5 = fcmp oeq float %v0, 2.0
  %ok6 = fcmp oeq float %v1, 3.0
  call void @__CPROVER_assert(i1 %ok5, ptr null)
  call void @__CPROVER_assert(i1 %ok6, ptr null)
  ret i32 0
}
