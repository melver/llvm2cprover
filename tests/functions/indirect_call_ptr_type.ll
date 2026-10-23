; ---
; description = "Indirect call where target takes typed ptr and call site passes opaque ptr"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare i1 @nondet_bool()
declare void @__CPROVER_assert(i1, ptr)

@vtable = internal constant { ptr, ptr } { ptr @target_typed, ptr @target_opaque }, align 8

define internal i32 @target_typed(ptr %state) {
entry:
  store i64 0, ptr %state, align 8
  ret i32 0
}

define internal i32 @target_opaque(ptr %state) {
entry:
  ret i32 0
}

define internal i32 @get_holder(ptr %in, ptr %out) {
entry:
  %c = call i1 @nondet_bool()
  br i1 %c, label %ok, label %err

ok:
  store ptr %in, ptr %out, align 8
  ret i32 0

err:
  ret i32 1
}

define i32 @main() {
entry:
  %state = alloca i64, align 8
  %auth = alloca { ptr, ptr }, align 8
  %slot = alloca ptr, align 8
  store i64 42, ptr %state, align 8
  store ptr @vtable, ptr %auth, align 8
  %state_field = getelementptr inbounds nuw i8, ptr %auth, i64 8
  store ptr %state, ptr %state_field, align 8
  store ptr null, ptr %slot, align 8

  %status = call i32 @get_holder(ptr %auth, ptr %slot)
  %failed = icmp ne i32 %status, 0
  br i1 %failed, label %done, label %call_it

call_it:
  %a = load ptr, ptr %slot, align 8
  %vt = load ptr, ptr %a, align 8
  %fn = load ptr, ptr %vt, align 8
  %st_ptr = getelementptr inbounds nuw i8, ptr %a, i64 8
  %st = load ptr, ptr %st_ptr, align 8
  %res = call i32 %fn(ptr %st)
  %ok = icmp eq i32 %res, 0
  call void @__CPROVER_assert(i1 %ok, ptr null)
  br label %done

done:
  ret i32 0
}
