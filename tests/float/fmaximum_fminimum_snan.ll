; ---
; description = "llvm.maximum/minimum(num) return a quiet NaN for signaling NaN inputs"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; LangRef: a NaN result is the preferred NaN or a quieted input payload, so the
; quiet bit (bit 51 for double, 22 for float) is always set.

declare double @llvm.maximum.f64(double, double)
declare double @llvm.minimum.f64(double, double)
declare double @llvm.maximumnum.f64(double, double)
declare double @llvm.minimumnum.f64(double, double)
declare float @llvm.maximum.f32(float, float)
declare void @__CPROVER_assert(i1, ptr)

define i1 @quiet_nan(double %x) {
  %b = bitcast double %x to i64
  %q = and i64 %b, 9221120237041090560
  %ok = icmp eq i64 %q, 9221120237041090560
  ret i1 %ok
}

define i32 @main() {
entry:
  ; A signaling NaN operand of maximum/minimum.
  %m0 = call double @llvm.maximum.f64(double 0x7FF4000000000000, double 1.0)
  %m1 = call double @llvm.minimum.f64(double 1.0, double 0x7FF4000000000000)
  ; Two signaling NaN operands of maximumnum/minimumnum.
  %m2 = call double @llvm.maximumnum.f64(double 0x7FF4000000000000, double 0x7FF4000000000000)
  %m3 = call double @llvm.minimumnum.f64(double 0x7FF4000000000000, double 0x7FF4000000000000)
  %ok0 = call i1 @quiet_nan(double %m0)
  %ok1 = call i1 @quiet_nan(double %m1)
  %ok2 = call i1 @quiet_nan(double %m2)
  %ok3 = call i1 @quiet_nan(double %m3)
  call void @__CPROVER_assert(i1 %ok0, ptr null)
  call void @__CPROVER_assert(i1 %ok1, ptr null)
  call void @__CPROVER_assert(i1 %ok2, ptr null)
  call void @__CPROVER_assert(i1 %ok3, ptr null)
  ; f32 (quiet bit 22): 0x7FA00000 in, 0x7FC00000 set in the result.
  %mf = call float @llvm.maximum.f32(float 0x7FF4000000000000, float 1.0)
  %bf = bitcast float %mf to i32
  %qf = and i32 %bf, 2143289344
  %okf = icmp eq i32 %qf, 2143289344
  call void @__CPROVER_assert(i1 %okf, ptr null)
  ret i32 0
}
