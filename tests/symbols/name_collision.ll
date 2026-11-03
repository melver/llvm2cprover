; ---
; description = "LLVM names that sanitize to the same GOTO identifier get distinct locals"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; `%.3.i`/`%_3.i` and `%a.b`/`%a_b` both sanitize to one identifier; this used to panic with
; "Tried to insert symbol which already existed".

declare void @__CPROVER_assert(i1, ptr)

define i64 @collide(i64 %a.b, i64 %a_b) {
entry:
  %_3.i = add i64 %a.b, 1
  %.3.i = add i64 %a_b, 2
  %ret = sub i64 %_3.i, %.3.i
  ret i64 %ret
}

define i32 @main() {
entry:
  %r = call i64 @collide(i64 10, i64 20)
  %ok = icmp eq i64 %r, -11
  call void @__CPROVER_assert(i1 %ok, ptr null)
  ret i32 0
}
