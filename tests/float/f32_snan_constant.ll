; ---
; description = "f32 constants keep their exact bits, including a signaling NaN"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; LLVM spells f32 constants in double format; converting through f64 and back
; would set the quiet bit of a signaling NaN.

declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  ; 0x7FA00000: signaling NaN.
  %b = bitcast float 0x7FF4000000000000 to i32
  %ok = icmp eq i32 %b, 2141192192
  call void @__CPROVER_assert(i1 %ok, ptr null)
  ret i32 0
}
