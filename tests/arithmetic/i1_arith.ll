; ---
; description = "i1 add/sub/mul/udiv/urem wrap mod 2 instead of being dropped on c_bool"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; `i1` lowers to `c_bool`, on which CBMC silently ignores arithmetic operators
; (`warning: ignoring +`). InstCombine usually rewrites these to xor/and, but
; unoptimized or hand-written IR keeps them.

declare i32 @__CPROVER_nondet_int()
declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %x = call i32 @__CPROVER_nondet_int()
  %y = call i32 @__CPROVER_nondet_int()
  %a = icmp sgt i32 %x, 0
  %b = icmp sgt i32 %y, 0
  %add = add i1 %a, %b
  %sub = sub i1 %a, %b
  %mul = mul i1 %a, %b
  %udiv = udiv i1 %a, true
  %urem = urem i1 %a, true
  %xor = xor i1 %a, %b
  %and = and i1 %a, %b
  %ok0 = icmp eq i1 %add, %xor
  %ok1 = icmp eq i1 %sub, %xor
  %ok2 = icmp eq i1 %mul, %and
  %ok3 = icmp eq i1 %udiv, %a
  %ok4 = icmp eq i1 %urem, false
  call void @__CPROVER_assert(i1 %ok0, ptr null)
  call void @__CPROVER_assert(i1 %ok1, ptr null)
  call void @__CPROVER_assert(i1 %ok2, ptr null)
  call void @__CPROVER_assert(i1 %ok3, ptr null)
  call void @__CPROVER_assert(i1 %ok4, ptr null)
  ret i32 0
}
