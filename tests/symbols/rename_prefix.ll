; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "rename-prefix preserves calls, function pointers, globals, and verifier builtins"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "old_main"
; translator_flags = ["--rename-prefix", "old_"]
; ---

@value = global i32 40
@value_ptr = global ptr @value
@add_ptr = global ptr @add_two
@message = private constant [14 x i8] c"renamed graph\00"

declare void @__CPROVER_assert(i1, ptr)
declare void @llvm.assume(i1)

define i32 @add_two(i32 %x) {
entry:
  %sum = add i32 %x, 2
  ret i32 %sum
}

define i32 @main() {
entry:
  call void @llvm.assume(i1 true)
  %global_address = load ptr, ptr @value_ptr
  %input = load i32, ptr %global_address
  %callee = load ptr, ptr @add_ptr
  %indirect = call i32 %callee(i32 %input)
  %direct = call i32 @add_two(i32 %input)
  %same = icmp eq i32 %indirect, %direct
  %answer = icmp eq i32 %direct, 42
  %ok = and i1 %same, %answer
  call void @__CPROVER_assert(i1 %ok, ptr @message)
  ret i32 0
}
