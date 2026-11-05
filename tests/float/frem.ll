; ---
; description = "frem lowers to CBMC's floatbv_mod, the expression behind its fmod model"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; esbmc_skip = "ESBMC's CBMC adapter lacks floatbv_mod"
; todo = "known CBMC bug (6.11.0): floatbv_mod computes lhs - (lhs / rhs) * rhs without truncating the quotient"
; ---
;
; frem is C's fmod: exact, with the sign of the dividend.

declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %a = frem double 5.5, 2.0
  %ok0 = fcmp oeq double %a, 1.5
  call void @__CPROVER_assert(i1 %ok0, ptr null)
  %b = frem float -7.0, 3.0
  %ok1 = fcmp oeq float %b, -1.0
  call void @__CPROVER_assert(i1 %ok1, ptr null)
  %v = frem <2 x double> <double 5.5, double -5.5>, <double 2.0, double 2.0>
  %v0 = extractelement <2 x double> %v, i64 0
  %v1 = extractelement <2 x double> %v, i64 1
  %ok2 = fcmp oeq double %v0, 1.5
  %ok3 = fcmp oeq double %v1, -1.5
  %ok23 = and i1 %ok2, %ok3
  call void @__CPROVER_assert(i1 %ok23, ptr null)
  ret i32 0
}
