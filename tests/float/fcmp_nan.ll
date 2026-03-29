; SPDX-License-Identifier: Apache-2.0
; Copyright (C) 2026, Marco Elver <me@marcoelver.com>
; ---
; description = "Ordered and unordered fcmp predicates preserve LLVM NaN semantics"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare void @__CPROVER_assert(i1, ptr)

@.msg1 = private unnamed_addr constant [13 x i8] c"uno sees nan\00"
@.msg2 = private unnamed_addr constant [16 x i8] c"ord rejects nan\00"
@.msg3 = private unnamed_addr constant [16 x i8] c"oeq rejects nan\00"
@.msg4 = private unnamed_addr constant [16 x i8] c"ueq accepts nan\00"

define i32 @main() {
entry:
  %nan = fdiv double 0.000000e+00, 0.000000e+00
  %uno = fcmp uno double %nan, 1.000000e+00
  %ord = fcmp ord double %nan, 1.000000e+00
  %oeq = fcmp oeq double %nan, 1.000000e+00
  %ueq = fcmp ueq double %nan, 1.000000e+00
  call void @__CPROVER_assert(i1 %uno, ptr @.msg1)
  %not_ord = xor i1 %ord, true
  call void @__CPROVER_assert(i1 %not_ord, ptr @.msg2)
  %not_oeq = xor i1 %oeq, true
  call void @__CPROVER_assert(i1 %not_oeq, ptr @.msg3)
  call void @__CPROVER_assert(i1 %ueq, ptr @.msg4)
  ret i32 0
}
