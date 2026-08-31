; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "Unwind-capable inline asm remains fail closed"
; expected = "TRANSLATION FAILED"
; ---

define void @model() {
entry:
  ret void
}

define void @main() {
entry:
  call void asm sideeffect unwind "opaque", ""()
  ret void
}
