; ---
; description = "loading an uninitialised alloca produces a nondet value"
; expected = "VERIFICATION FAILED"
; entry = "main"
; ---

@.msg = private unnamed_addr constant [12 x i8] c"uninit read\00"

declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %x = alloca i32, align 4
  ; No store to %x — the slot is uninitialised.
  %val = load i32, ptr %x
  ; CBMC must explore the case where %val != 42, so the assertion fails.
  %ok = icmp eq i32 %val, 42
  call void @__CPROVER_assert(i1 %ok, ptr @.msg)
  ret i32 0
}
