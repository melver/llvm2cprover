; ---
; description = "Byte GEP instruction into a one-element struct-array global where the target field is not the last"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

%struct.three = type { i64, i64, i64 }

@s = internal global [1 x %struct.three] zeroinitializer, align 8

@.msg = private unnamed_addr constant [18 x i8] c"byte gep mid read\00"

declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %ptr = getelementptr inbounds i8, ptr @s, i64 8
  store i64 99, ptr %ptr, align 8
  %val = load i64, ptr %ptr, align 8
  %ok = icmp eq i64 %val, 99
  call void @__CPROVER_assert(i1 %ok, ptr @.msg)
  ret i32 0
}
