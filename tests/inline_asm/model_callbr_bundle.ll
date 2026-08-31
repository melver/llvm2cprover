; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "Modeled callbr operand bundles remain fail closed"
; expected = "TRANSLATION FAILED"
; ---

define void @model_goto() {
entry:
  ret void
}

define void @main() {
entry:
  callbr void asm sideeffect "opaque goto ${0:l}", "!i"() [ "unknown"() ]
      to label %fallthrough [label %indirect]

fallthrough:
  ret void

indirect:
  ret void
}
