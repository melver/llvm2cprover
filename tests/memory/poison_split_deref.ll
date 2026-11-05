; ---
; description = "A poison ?: (shift by a symbolic amount) followed by derefs passes run.py --goto-convert"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; CBMC splits a ?: whose branch is nondet into control flow, which restarts goto_check's
; deduplication of the later dereference checks; --goto-convert keeps it inline. The property sets
; then differ only in duplicate checks and numbering, which run.py's comparison must tolerate.

declare i32 @__CPROVER_nondet_int()
declare void @__CPROVER_assume(i1)
declare void @__CPROVER_assert(i1, ptr)

define void @shift_in_place(ptr %p, i32 %n) {
entry:
  %x = load i32, ptr %p
  %y = shl i32 %x, %n
  store i32 %y, ptr %p
  ret void
}

define i32 @main() {
entry:
  %cell = alloca i32
  store i32 3, ptr %cell
  %n = call i32 @__CPROVER_nondet_int()
  %ok.n = icmp ult i32 %n, 4
  call void @__CPROVER_assume(i1 %ok.n)
  call void @shift_in_place(ptr %cell, i32 %n)
  %v = load i32, ptr %cell
  %ok = icmp ugt i32 %v, 2
  call void @__CPROVER_assert(i1 %ok, ptr null)
  ret i32 0
}
