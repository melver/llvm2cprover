; ---
; description = "sitofp/fptosi treat integers as signed, including negative and i1 values"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; Integers lower to unsigned bitvectors, so a plain cast reads negative sources (sitofp) and
; produces negative results (fptosi) as unsigned values.

declare i16 @__CPROVER_nondet_short()
declare void @__CPROVER_assume(i1)
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %x = call i16 @__CPROVER_nondet_short()
  %neg = icmp eq i16 %x, -17446
  call void @__CPROVER_assume(i1 %neg)
  %f = sitofp i16 %x to float
  %ok0 = fcmp oeq float %f, -17446.0
  call void @__CPROVER_assert(i1 %ok0, ptr null)
  %b = sitofp i1 true to double
  %ok1 = fcmp oeq double %b, -1.0
  call void @__CPROVER_assert(i1 %ok1, ptr null)
  %v = sitofp <2 x i8> <i8 -1, i8 2> to <2 x float>
  %v0 = extractelement <2 x float> %v, i64 0
  %ok2 = fcmp oeq float %v0, -1.0
  call void @__CPROVER_assert(i1 %ok2, ptr null)
  %i = fptosi float %f to i32
  %ok3 = icmp eq i32 %i, -17446
  call void @__CPROVER_assert(i1 %ok3, ptr null)
  %j = fptosi double -3.5 to i8
  %ok4 = icmp eq i8 %j, -3
  call void @__CPROVER_assert(i1 %ok4, ptr null)
  ret i32 0
}
