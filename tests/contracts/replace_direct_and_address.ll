; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "a direct call cannot also pass the selected target as an argument"
; expected = "TRANSLATION FAILED"
; translator_flags = ["--replace-with-contract", "target"]
; ---

define void @target(ptr %callback) {
entry:
  ret void
}

define i32 @main() {
entry:
  call void @target(ptr @target)
  ret i32 0
}
