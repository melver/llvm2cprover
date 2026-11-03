; ---
; description = "scalar float/integer bitcasts preserve IEEE-754 bit patterns"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-linux-gnu"

declare void @__CPROVER_assert(i1, ptr)
declare float @__CPROVER_nondet_float()

@.f32 = private unnamed_addr constant [4 x i8] c"f32\00"
@.f64 = private unnamed_addr constant [4 x i8] c"f64\00"
@.any = private unnamed_addr constant [4 x i8] c"any\00"

define i32 @main() {
entry:
  %x = call float @__CPROVER_nondet_float()
  %xbits = bitcast float %x to i32
  %xback = bitcast i32 %xbits to float
  %xbits2 = bitcast float %xback to i32
  %ok.any = icmp eq i32 %xbits2, %xbits                 ; every value, NaNs included
  call void @__CPROVER_assert(i1 %ok.any, ptr @.any)

  %one = fadd float 1.000000e+00, 0.000000e+00
  %bits = bitcast float %one to i32
  %roundtrip = bitcast i32 %bits to float
  %ok.f32.1 = icmp eq i32 %bits, 1065353216             ; 0x3f800000
  %ok.f32.2 = fcmp oeq float %roundtrip, 1.000000e+00
  %ok.f32 = and i1 %ok.f32.1, %ok.f32.2
  call void @__CPROVER_assert(i1 %ok.f32, ptr @.f32)

  %neg_two = fsub double -2.000000e+00, 0.000000e+00
  %dbits = bitcast double %neg_two to i64
  %dround = bitcast i64 %dbits to double
  %ok.f64.1 = icmp eq i64 %dbits, -4611686018427387904 ; 0xc000000000000000
  %ok.f64.2 = fcmp oeq double %dround, -2.000000e+00
  %ok.f64 = and i1 %ok.f64.1, %ok.f64.2
  call void @__CPROVER_assert(i1 %ok.f64, ptr @.f64)
  ret i32 0
}
