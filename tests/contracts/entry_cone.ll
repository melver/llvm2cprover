; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "--entry restricts lowering to the contract-aware cone of influence"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; translator_flags = ["--entry", "main"]
; ---

@unused_str = private unnamed_addr constant [8 x i8] c"ignored\00"

declare void @__CPROVER_assert(i1, ptr)

define void @unreachable_with_asm() {
entry:
  %p = getelementptr [8 x i8], ptr @unused_str, i64 0, i64 0
  call void asm sideeffect "ud2", ""()
  ret void
}

define i32 @step_with_unsupported_body(i32 %x) {
entry:
  call void @unreachable_with_asm()
  ret i32 %x
}

define i32 @model_step(i32 %x) {
entry:
  %next = add i32 %x, 1
  ret i32 %next
}

define i32 @main() {
entry:
  %res = call i32 @step_with_unsupported_body(i32 41)
  %ok = icmp eq i32 %res, 42
  call void @__CPROVER_assert(i1 %ok, ptr null)
  ret i32 0
}
