; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "A callbr model runs once and preserves result and PHI semantics on every edge"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

@state = global i32 0

declare void @__CPROVER_assert(i1, ptr)

define i32 @model_goto(i32 %value) {
entry:
  %old.state = load i32, ptr @state
  %new.state = add i32 %old.state, 1
  store i32 %new.state, ptr @state
  %result = add i32 %value, 1
  ret i32 %result
}

define i32 @main() {
entry:
  %result = callbr i32 asm sideeffect "opaque goto ${2:l} ${3:l}", "=r,r,!i,!i"(i32 41)
      to label %fallthrough [label %first, label %second]

fallthrough:
  %fallthrough.result = phi i32 [ %result, %entry ]
  %fallthrough.state = load i32, ptr @state
  %fallthrough.result.ok = icmp eq i32 %fallthrough.result, 42
  %fallthrough.state.ok = icmp eq i32 %fallthrough.state, 1
  %fallthrough.ok = and i1 %fallthrough.result.ok, %fallthrough.state.ok
  call void @__CPROVER_assert(i1 %fallthrough.ok, ptr null)
  br label %exit

first:
  %first.result = phi i32 [ %result, %entry ]
  %first.state = load i32, ptr @state
  %first.result.ok = icmp eq i32 %first.result, 42
  %first.state.ok = icmp eq i32 %first.state, 1
  %first.ok = and i1 %first.result.ok, %first.state.ok
  call void @__CPROVER_assert(i1 %first.ok, ptr null)
  br label %exit

second:
  %second.result = phi i32 [ %result, %entry ]
  %second.state = load i32, ptr @state
  %second.result.ok = icmp eq i32 %second.result, 42
  %second.state.ok = icmp eq i32 %second.state, 1
  %second.ok = and i1 %second.result.ok, %second.state.ok
  call void @__CPROVER_assert(i1 %second.ok, ptr null)
  br label %exit

exit:
  %path = phi i32 [ 0, %fallthrough ], [ 1, %first ], [ 2, %second ]
  %path.ok = icmp ult i32 %path, 3
  call void @__CPROVER_assert(i1 %path.ok, ptr null)
  ret i32 0
}
