; ---
; description = "llvm.fmuladd matches x86 codegen: unfused without the FMA feature, fused with it"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; a*b = 1 + 2^-11 + 2^-24 exactly; rounded to float it ties to even at 1 + 2^-11, so the unfused
; result is 0 while the fused one is 2^-24.

target triple = "x86_64-pc-linux-gnu"

declare float @llvm.fmuladd.f32(float, float, float)
declare void @__CPROVER_assert(i1, ptr)

define float @unfused(float %a, float %b, float %c) {
  %r = call float @llvm.fmuladd.f32(float %a, float %b, float %c)
  ret float %r
}

define float @fused(float %a, float %b, float %c) #0 {
  %r = call float @llvm.fmuladd.f32(float %a, float %b, float %c)
  ret float %r
}

define i32 @main() {
entry:
  %u = call float @unfused(float 0x3FF0010000000000, float 0x3FF0010000000000, float 0xBFF0020000000000)
  %f = call float @fused(float 0x3FF0010000000000, float 0x3FF0010000000000, float 0xBFF0020000000000)
  %ok0 = fcmp oeq float %u, 0.0
  %ok1 = fcmp oeq float %f, 0x3E70000000000000
  call void @__CPROVER_assert(i1 %ok0, ptr null)
  call void @__CPROVER_assert(i1 %ok1, ptr null)
  ret i32 0
}

attributes #0 = { "target-features"="+avx,+fma,+sse2" }
