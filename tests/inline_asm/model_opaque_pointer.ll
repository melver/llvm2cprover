; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "Inline asm models accept compatible LLVM opaque pointers"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare void @__CPROVER_assert(i1, ptr)

define ptr @model_identity(ptr %value) {
entry:
  %byte = load i8, ptr %value
  ret ptr %value
}

define i32 @main() {
entry:
  %slot = alloca i32
  store i32 0, ptr %slot
  %result = call ptr asm sideeffect "opaque pointer", "=r,r"(ptr %slot)
  %ok = icmp eq ptr %result, %slot
  call void @__CPROVER_assert(i1 %ok, ptr null)
  ret i32 0
}
