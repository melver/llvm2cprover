; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "contract replacement rejects invoke call sites"
; expected = "TRANSLATION FAILED"
; translator_flags = ["--replace-with-contract", "target"]
; ---

declare i32 @__gxx_personality_v0(...)

define i32 @target() {
entry:
  ret i32 1
}

define i32 @main() personality ptr @__gxx_personality_v0 {
entry:
  %result = invoke i32 @target()
          to label %normal unwind label %cleanup
normal:
  ret i32 %result
cleanup:
  %landing = landingpad { ptr, i32 } cleanup
  ret i32 0
}
