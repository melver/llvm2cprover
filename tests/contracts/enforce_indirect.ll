; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "contract enforcement rejects address-taken targets"
; expected = "TRANSLATION FAILED"
; translator_flags = ["--enforce-contract", "target"]
; ---

define i32 @target(i32 %x) {
entry:
  ret i32 %x
}

define i32 @main() {
entry:
  %result = call i32 @call_indirect(ptr @target, i32 1)
  ret i32 %result
}

define i32 @call_indirect(ptr %callee, i32 %x) {
entry:
  %result = call i32 %callee(i32 %x)
  ret i32 %result
}
