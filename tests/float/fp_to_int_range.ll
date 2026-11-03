; ---
; description = "fptosi/fptoui convert values whose truncation fits the target exactly, at the range boundaries"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; Truncation toward zero maps (lo - 1, hi) into the target range, so e.g. -2^31 - 0.5 is a valid
; fptosi-to-i32 input while 2^31 is not (that one is covered by fp_to_int_poison).

declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %a = fptosi double -2147483648.5 to i32
  %ok0 = icmp eq i32 %a, -2147483648
  call void @__CPROVER_assert(i1 %ok0, ptr null)
  %b = fptosi float -2147483648.0 to i32
  %ok1 = icmp eq i32 %b, -2147483648
  call void @__CPROVER_assert(i1 %ok1, ptr null)
  %c = fptosi double 2147483647.5 to i32
  %ok2 = icmp eq i32 %c, 2147483647
  call void @__CPROVER_assert(i1 %ok2, ptr null)
  %d = fptoui double -0.5 to i32
  %ok3 = icmp eq i32 %d, 0
  call void @__CPROVER_assert(i1 %ok3, ptr null)
  %e = fptoui double 4294967295.5 to i32
  %ok4 = icmp eq i32 %e, -1
  call void @__CPROVER_assert(i1 %ok4, ptr null)
  %f = fptosi float -128.5 to i8
  %ok5 = icmp eq i8 %f, -128
  call void @__CPROVER_assert(i1 %ok5, ptr null)
  ret i32 0
}
