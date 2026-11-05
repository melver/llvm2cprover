; ---
; description = "Vector float intrinsics lower lane-wise: libm builtins per lane, unfused fmuladd as vector arithmetic"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; esbmc_skip = "ESBMC 8.6 crashes in Bitwuzla"
; ---
;
; The loop vectorizer emits these for Csmith --float programs (seeds 87, 303, 938). The fmuladd
; inputs are those of fmuladd_x86: unfused gives 0, fused (+fma) gives 2^-24.

target triple = "x86_64-pc-linux-gnu"

declare <4 x float> @llvm.fabs.v4f32(<4 x float>)
declare <2 x double> @llvm.copysign.v2f64(<2 x double>, <2 x double>)
declare <2 x float> @llvm.fmuladd.v2f32(<2 x float>, <2 x float>, <2 x float>)
declare i1 @llvm.vector.reduce.and.v4i1(<4 x i1>)
declare i1 @llvm.vector.reduce.and.v2i1(<2 x i1>)
declare void @__CPROVER_assert(i1, ptr)

define <2 x float> @unfused(<2 x float> %a, <2 x float> %b, <2 x float> %c) {
  %r = call <2 x float> @llvm.fmuladd.v2f32(<2 x float> %a, <2 x float> %b, <2 x float> %c)
  ret <2 x float> %r
}

define <2 x float> @fused(<2 x float> %a, <2 x float> %b, <2 x float> %c) #0 {
  %r = call <2 x float> @llvm.fmuladd.v2f32(<2 x float> %a, <2 x float> %b, <2 x float> %c)
  ret <2 x float> %r
}

define i32 @main() {
entry:
  %abs = call <4 x float> @llvm.fabs.v4f32(<4 x float> <float -1.5, float 2.0, float -0.0, float -3.0>)
  %abs_eq = fcmp oeq <4 x float> %abs, <float 1.5, float 2.0, float 0.0, float 3.0>
  %abs_ok = call i1 @llvm.vector.reduce.and.v4i1(<4 x i1> %abs_eq)
  call void @__CPROVER_assert(i1 %abs_ok, ptr null)

  %cs = call <2 x double> @llvm.copysign.v2f64(<2 x double> <double 1.0, double -2.0>, <2 x double> <double -0.0, double 1.0>)
  %cs_eq = fcmp oeq <2 x double> %cs, <double -1.0, double 2.0>
  %cs_ok = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %cs_eq)
  call void @__CPROVER_assert(i1 %cs_ok, ptr null)

  %u = call <2 x float> @unfused(<2 x float> <float 0x3FF0010000000000, float 2.0>, <2 x float> <float 0x3FF0010000000000, float 3.0>, <2 x float> <float 0xBFF0020000000000, float 1.0>)
  %u_eq = fcmp oeq <2 x float> %u, <float 0.0, float 7.0>
  %u_ok = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %u_eq)
  call void @__CPROVER_assert(i1 %u_ok, ptr null)

  %f = call <2 x float> @fused(<2 x float> <float 0x3FF0010000000000, float 2.0>, <2 x float> <float 0x3FF0010000000000, float 3.0>, <2 x float> <float 0xBFF0020000000000, float 1.0>)
  %f_eq = fcmp oeq <2 x float> %f, <float 0x3E70000000000000, float 7.0>
  %f_ok = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %f_eq)
  call void @__CPROVER_assert(i1 %f_ok, ptr null)
  ret i32 0
}

attributes #0 = { "target-features"="+avx,+fma,+sse2" }
