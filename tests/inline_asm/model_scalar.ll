; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "Compiled model replaces an exactly matched inline asm call"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare void @__CPROVER_assert(i1, ptr)

define i32 @model_increment(i32 %value) {
entry:
  %result = add i32 %value, 1
  ret i32 %result
}

define i32 @main() {
entry:
  %result = call i32 asm sideeffect "opaque $0", "=r,r"(i32 41)
  %ok = icmp eq i32 %result, 42
  call void @__CPROVER_assert(i1 %ok, ptr null)
  ret i32 0
}
