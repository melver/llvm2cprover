; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "Empty asm-goto does not use the ordinary empty-assembly shortcut"
; expected = "TRANSLATION FAILED"
; ---

define void @main() {
entry:
  callbr void asm sideeffect "", "!i"()
      to label %fallthrough [label %indirect]

fallthrough:
  ret void

indirect:
  ret void
}
