; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "Inline asm models preserve multi-output aggregate results"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare void @__CPROVER_assert(i1, ptr)

define { i32, i32 } @model_pair(i32 %value) {
entry:
  %first = insertvalue { i32, i32 } poison, i32 %value, 0
  %second = insertvalue { i32, i32 } %first, i32 9, 1
  ret { i32, i32 } %second
}

define i32 @main() {
entry:
  %pair = call { i32, i32 } asm sideeffect "opaque pair", "=r,=r,r"(i32 7)
  %left = extractvalue { i32, i32 } %pair, 0
  %right = extractvalue { i32, i32 } %pair, 1
  %left_ok = icmp eq i32 %left, 7
  %right_ok = icmp eq i32 %right, 9
  %ok = and i1 %left_ok, %right_ok
  call void @__CPROVER_assert(i1 %ok, ptr null)
  ret i32 0
}
