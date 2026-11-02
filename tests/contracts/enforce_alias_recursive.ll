; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "contract enforcement rejects recursive call cycles routed through a GlobalAlias"
; expected = "TRANSLATION FAILED"
; translator_flags = ["--entry", "target", "--enforce-contract", "target"]
; ---

@rec_alias = unnamed_addr alias i32 (i32), ptr @target

define i32 @target(i32 %x) {
entry:
  %done = icmp eq i32 %x, 0
  br i1 %done, label %base, label %step
base:
  ret i32 0
step:
  %next = sub i32 %x, 1
  %r = call i32 @rec_alias(i32 %next)
  ret i32 %r
}
