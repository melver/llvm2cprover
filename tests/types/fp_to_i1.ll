; ---
; description = "fptoui/fptosi to i1 produce the low bit rather than crashing on a c_bool cast"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; `i1` lowers to `c_bool`, which CBMC cannot convert a float to directly.
; In range, fptoui yields 0/1 and fptosi yields 0/-1 (bit pattern 1).

declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %u1 = fptoui double 1.0 to i1
  %u0 = fptoui double 0.0 to i1
  %s1 = fptosi double -1.0 to i1
  %sv = fptosi <2 x float> <float 0.0, float -1.0> to <2 x i1>
  %sv0 = extractelement <2 x i1> %sv, i64 0
  %sv1 = extractelement <2 x i1> %sv, i64 1
  %n0 = xor i1 %u0, true
  %n2 = xor i1 %sv0, true
  %a0 = and i1 %u1, %n0
  %a1 = and i1 %a0, %s1
  %a2 = and i1 %a1, %n2
  %a3 = and i1 %a2, %sv1
  call void @__CPROVER_assert(i1 %a3, ptr null)
  ret i32 0
}
