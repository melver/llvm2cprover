; SPDX-License-Identifier: Apache-2.0
; Copyright 2026 The llvm2cprover Authors
;
; ---
; description = "snapshot postcondition is checked on an invoke normal edge"
; expected = "VERIFICATION FAILED"
; entry = "main"
; ---

declare i32 @__gxx_personality_v0(...)

define i32 @snapshot_increment(ptr %counter) {
entry:
  %before = load i32, ptr %counter
  ret i32 %before
}

define i32 @increment(ptr %counter) {
entry:
  %before = load i32, ptr %counter
  %after = add i32 %before, 2
  store i32 %after, ptr %counter
  ret i32 %after
}

define i1 @ensures_increment(ptr %counter, i32 %before, i32 %result) {
entry:
  %after = load i32, ptr %counter
  %expected = add i32 %before, 1
  %state.ok = icmp eq i32 %after, %expected
  %result.ok = icmp eq i32 %result, %after
  %ok = and i1 %state.ok, %result.ok
  ret i1 %ok
}

define i32 @main() personality ptr @__gxx_personality_v0 {
entry:
  %counter = alloca i32
  store i32 41, ptr %counter
  %result = invoke i32 @increment(ptr %counter)
          to label %normal unwind label %cleanup
normal:
  ret i32 0
cleanup:
  %landing = landingpad { ptr, i32 } cleanup
  ret i32 0
}
