; ---
; description = "Vector fcmp compares lane-wise, with per-lane NaN handling"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; This used to panic in cprover_bindings: `isnan` was applied to the whole vector.

declare i1 @llvm.vector.reduce.and.v4i1(<4 x i1>)
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %oeq = fcmp oeq <4 x float> <float 1.0, float 0x7FF8000000000000, float 2.0, float -0.0>, <float 1.0, float 0x7FF8000000000000, float 3.0, float 0.0>
  %oeq_ok = icmp eq <4 x i1> %oeq, <i1 true, i1 false, i1 false, i1 true>
  %a = call i1 @llvm.vector.reduce.and.v4i1(<4 x i1> %oeq_ok)
  call void @__CPROVER_assert(i1 %a, ptr null)
  %ult = fcmp ult <4 x double> <double 1.0, double 0x7FF8000000000000, double 4.0, double 2.0>, <double 2.0, double 0.0, double 3.0, double 2.0>
  %ult_ok = icmp eq <4 x i1> %ult, <i1 true, i1 true, i1 false, i1 false>
  %b = call i1 @llvm.vector.reduce.and.v4i1(<4 x i1> %ult_ok)
  call void @__CPROVER_assert(i1 %b, ptr null)
  ret i32 0
}
