; ---
; description = "llvm.is.fpclass tests each of the ten IEEE classes, lane-wise for vectors"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare i1 @llvm.is.fpclass.f64(double, i32)
declare i1 @llvm.is.fpclass.f32(float, i32)
declare <2 x i1> @llvm.is.fpclass.v2f32(<2 x float>, i32)
declare float @__CPROVER_nondet_float()
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  ; Each double is in exactly its own class: mask bit k (k = 0..9) is
  ; sNaN, qNaN, -inf, -normal, -subnormal, -zero, +zero, +subnormal, +normal, +inf.
  ; sNaN
  %in0 = call i1 @llvm.is.fpclass.f64(double 0x7FF4000000000000, i32 1)
  %out0 = call i1 @llvm.is.fpclass.f64(double 0x7FF4000000000000, i32 1022)
  %nout0 = xor i1 %out0, true
  %ok0 = and i1 %in0, %nout0
  call void @__CPROVER_assert(i1 %ok0, ptr null)
  ; qNaN
  %in1 = call i1 @llvm.is.fpclass.f64(double 0x7FF8000000000000, i32 2)
  %out1 = call i1 @llvm.is.fpclass.f64(double 0x7FF8000000000000, i32 1021)
  %nout1 = xor i1 %out1, true
  %ok1 = and i1 %in1, %nout1
  call void @__CPROVER_assert(i1 %ok1, ptr null)
  ; -inf
  %in2 = call i1 @llvm.is.fpclass.f64(double 0xFFF0000000000000, i32 4)
  %out2 = call i1 @llvm.is.fpclass.f64(double 0xFFF0000000000000, i32 1019)
  %nout2 = xor i1 %out2, true
  %ok2 = and i1 %in2, %nout2
  call void @__CPROVER_assert(i1 %ok2, ptr null)
  ; -normal
  %in3 = call i1 @llvm.is.fpclass.f64(double -1.0, i32 8)
  %out3 = call i1 @llvm.is.fpclass.f64(double -1.0, i32 1015)
  %nout3 = xor i1 %out3, true
  %ok3 = and i1 %in3, %nout3
  call void @__CPROVER_assert(i1 %ok3, ptr null)
  ; -subnormal
  %in4 = call i1 @llvm.is.fpclass.f64(double 0x8000000000000001, i32 16)
  %out4 = call i1 @llvm.is.fpclass.f64(double 0x8000000000000001, i32 1007)
  %nout4 = xor i1 %out4, true
  %ok4 = and i1 %in4, %nout4
  call void @__CPROVER_assert(i1 %ok4, ptr null)
  ; -zero
  %in5 = call i1 @llvm.is.fpclass.f64(double -0.0, i32 32)
  %out5 = call i1 @llvm.is.fpclass.f64(double -0.0, i32 991)
  %nout5 = xor i1 %out5, true
  %ok5 = and i1 %in5, %nout5
  call void @__CPROVER_assert(i1 %ok5, ptr null)
  ; +zero
  %in6 = call i1 @llvm.is.fpclass.f64(double 0.0, i32 64)
  %out6 = call i1 @llvm.is.fpclass.f64(double 0.0, i32 959)
  %nout6 = xor i1 %out6, true
  %ok6 = and i1 %in6, %nout6
  call void @__CPROVER_assert(i1 %ok6, ptr null)
  ; +subnormal
  %in7 = call i1 @llvm.is.fpclass.f64(double 0x0000000000000001, i32 128)
  %out7 = call i1 @llvm.is.fpclass.f64(double 0x0000000000000001, i32 895)
  %nout7 = xor i1 %out7, true
  %ok7 = and i1 %in7, %nout7
  call void @__CPROVER_assert(i1 %ok7, ptr null)
  ; +normal
  %in8 = call i1 @llvm.is.fpclass.f64(double 1.0, i32 256)
  %out8 = call i1 @llvm.is.fpclass.f64(double 1.0, i32 767)
  %nout8 = xor i1 %out8, true
  %ok8 = and i1 %in8, %nout8
  call void @__CPROVER_assert(i1 %ok8, ptr null)
  ; +inf
  %in9 = call i1 @llvm.is.fpclass.f64(double 0x7FF0000000000000, i32 512)
  %out9 = call i1 @llvm.is.fpclass.f64(double 0x7FF0000000000000, i32 511)
  %nout9 = xor i1 %out9, true
  %ok9 = and i1 %in9, %nout9
  call void @__CPROVER_assert(i1 %ok9, ptr null)

  ; f32: a signaling NaN is not quiet; mask 3 (any NaN) matches `fcmp uno` for every value.
  %fs = call i1 @llvm.is.fpclass.f32(float 0x7FF4000000000000, i32 1)
  call void @__CPROVER_assert(i1 %fs, ptr null)
  %x = call float @__CPROVER_nondet_float()
  %x_nan = call i1 @llvm.is.fpclass.f32(float %x, i32 3)
  %x_uno = fcmp uno float %x, %x
  %nan_ok = icmp eq i1 %x_nan, %x_uno
  call void @__CPROVER_assert(i1 %nan_ok, ptr null)
  ; Mask 0 is always false.
  %none = call i1 @llvm.is.fpclass.f32(float %x, i32 0)
  %none_ok = xor i1 %none, true
  call void @__CPROVER_assert(i1 %none_ok, ptr null)

  ; Lane-wise: +inf (mask 512 | 4: any infinity) and 1.0.
  %v = call <2 x i1> @llvm.is.fpclass.v2f32(<2 x float> <float 0x7FF0000000000000, float 1.0>, i32 516)
  %v0 = extractelement <2 x i1> %v, i32 0
  %v1 = extractelement <2 x i1> %v, i32 1
  %nv1 = xor i1 %v1, true
  %vok = and i1 %v0, %nv1
  call void @__CPROVER_assert(i1 %vok, ptr null)
  ret i32 0
}
