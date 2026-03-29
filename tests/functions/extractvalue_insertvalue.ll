; ---
; description = "Aggregate return values can be decomposed and rebuilt"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

declare void @__CPROVER_assert(i1, ptr)

@.msg = private unnamed_addr constant [10 x i8] c"aggregate\00"

define { i32, i32 } @make_pair(i32 %x, i32 %y) {
entry:
  %pair0 = insertvalue { i32, i32 } undef, i32 %x, 0
  %pair1 = insertvalue { i32, i32 } %pair0, i32 %y, 1
  ret { i32, i32 } %pair1
}

define i32 @main() {
entry:
  %pair = call { i32, i32 } @make_pair(i32 2, i32 9)
  %x = extractvalue { i32, i32 } %pair, 0
  %y = extractvalue { i32, i32 } %pair, 1
  %okx = icmp eq i32 %x, 2
  %oky = icmp eq i32 %y, 9
  %ok = and i1 %okx, %oky
  call void @__CPROVER_assert(i1 %ok, ptr @.msg)
  ret i32 0
}
