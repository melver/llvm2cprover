; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "A modeled callbr conservatively considers its indirect destination"
; expected = "VERIFICATION FAILED"
; entry = "main"
; ---

declare void @__CPROVER_assert(i1, ptr)

define void @model_goto(i32 %value) {
entry:
  ret void
}

define void @main() {
entry:
  callbr void asm sideeffect "opaque goto ${1:l}", "r,!i"(i32 0)
      to label %fallthrough [label %indirect]

fallthrough:
  ret void

indirect:
  call void @__CPROVER_assert(i1 false, ptr null)
  ret void
}
