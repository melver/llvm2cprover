; ---
; description = "floating-point vector reductions stay unsupported and fail translation"
; expected = "TRANSLATION FAILED"
; entry = "main"
; ---

declare float @llvm.vector.reduce.fadd.v4f32(float, <4 x float>)

define i32 @main() {
entry:
  %r = call float @llvm.vector.reduce.fadd.v4f32(float 0.0, <4 x float> zeroinitializer)
  %i = fptosi float %r to i32
  ret i32 %i
}
