; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "Unmodeled asm-goto remains fail closed"
; expected = "TRANSLATION FAILED"
; ---

define void @main() {
entry:
  callbr void asm sideeffect "opaque goto ${0:l}", "!i"()
      to label %fallthrough [label %indirect]

fallthrough:
  ret void

indirect:
  ret void
}
