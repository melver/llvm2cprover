; ---
; description = "i1 values returned by an external function or __CPROVER_nondet_* are 0 or 1"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; cbmc_flags = ["--no-unwinding-assertions"]
; esbmc_skip = "exercises CBMC's havoc of a bodiless callee's return value"
; ---
;
; CBMC 6.11 leaves c_bool unconstrained in a bodiless function's nondet return
; value (its `nondet_*` handling constrains only a scalar). An i1 read as true by
; a branch must also be exactly `true` for icmp/xor. --no-unwinding-assertions
; drops CBMC's "no body for callee" property, which `ext_bool` would fail.

declare <4 x i1> @nondet_mask()
declare { i32, i1 } @nondet_pair()
declare i1 @ext_bool()
declare <4 x i1> @__CPROVER_nondet_mask()
declare void @__CPROVER_assert(i1, ptr)

define void @check(i1 %b) {
entry:
  br i1 %b, label %set, label %done
set:
  %not = xor i1 %b, true
  %ok = icmp eq i1 %not, false
  call void @__CPROVER_assert(i1 %ok, ptr null)
  br label %done
done:
  ret void
}

define i32 @main() {
entry:
  %m = call <4 x i1> @nondet_mask()
  %lane = extractelement <4 x i1> %m, i64 2
  call void @check(i1 %lane)
  %p = call { i32, i1 } @nondet_pair()
  %field = extractvalue { i32, i1 } %p, 1
  call void @check(i1 %field)
  %b = call i1 @ext_bool()
  call void @check(i1 %b)
  %c = call <4 x i1> @__CPROVER_nondet_mask()
  %clane = extractelement <4 x i1> %c, i64 1
  call void @check(i1 %clane)
  ret i32 0
}
