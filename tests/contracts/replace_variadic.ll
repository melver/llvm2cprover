; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "contract replacement summarizes fixed parameters of a variadic extern"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; translator_flags = ["--replace-with-contract", "target"]
; ---

declare i32 @target(i32, ...)
declare void @__CPROVER_assert(i1, ptr)

define i1 @contract_ensures_target(i32 %fixed, i32 %result) {
entry:
  %expected = add i32 %fixed, 1
  %same = icmp eq i32 %result, %expected
  ret i1 %same
}

define i32 @main() {
entry:
  %result = call i32 (i32, ...) @target(i32 6, i32 99, ptr null)
  %ok = icmp eq i32 %result, 7
  call void @__CPROVER_assert(i1 %ok, ptr null)
  ret i32 0
}
