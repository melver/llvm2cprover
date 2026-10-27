; ---
; description = "GEPs that step out of a subarray must address the neighbouring element, not index the subarray value"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-linux-gnu"

declare void @__CPROVER_assert(i1, ptr)

define internal i8 @read_via_decayed_base(ptr %rounds, i64 %i) {
entry:
  %row_i = getelementptr [16 x i8], ptr %rounds, i64 %i, i64 0
  %prev = getelementptr i8, ptr %row_i, i64 -4
  %val = load i8, ptr %prev, align 1
  ret i8 %val
}

define internal i8 @read_via_two_index_gep(ptr %rounds, i64 %i) {
entry:
  %prev = getelementptr [16 x i8], ptr %rounds, i64 %i, i64 -4
  %val = load i8, ptr %prev, align 1
  ret i8 %val
}

define internal void @write_via_two_index_gep(ptr %rounds, i64 %i, i8 %v) {
entry:
  %prev = getelementptr [16 x i8], ptr %rounds, i64 %i, i64 -4
  store i8 %v, ptr %prev, align 1
  ret void
}

; Positive offset past the end of row 0 lands in row 1.
define internal i8 @read_past_row_end(ptr %rounds) {
entry:
  %next = getelementptr [16 x i8], ptr %rounds, i64 0, i64 20
  %val = load i8, ptr %next, align 1
  ret i8 %val
}

; Negative index into a struct's array field lands in the preceding i32 field.
define internal i8 @read_struct_field_underflow(ptr %s) {
entry:
  %prev = getelementptr { i32, [4 x i8] }, ptr %s, i64 0, i32 1, i64 -4
  %val = load i8, ptr %prev, align 1
  ret i8 %val
}

define i32 @main() {
entry:
  %buf = alloca [2 x [16 x i8]], align 16
  %target = getelementptr inbounds [2 x [16 x i8]], ptr %buf, i64 0, i64 0, i64 12
  store i8 42, ptr %target, align 1
  %a = call i8 @read_via_decayed_base(ptr %buf, i64 1)
  %b = call i8 @read_via_two_index_gep(ptr %buf, i64 1)
  %ok1 = icmp eq i8 %a, 42
  %ok2 = icmp eq i8 %b, 42
  call void @__CPROVER_assert(i1 %ok1, ptr null)
  call void @__CPROVER_assert(i1 %ok2, ptr null)
  call void @write_via_two_index_gep(ptr %buf, i64 1, i8 99)
  %c = load i8, ptr %target, align 1
  %ok3 = icmp eq i8 %c, 99
  call void @__CPROVER_assert(i1 %ok3, ptr null)

  %row1_4 = getelementptr inbounds [2 x [16 x i8]], ptr %buf, i64 0, i64 1, i64 4
  store i8 17, ptr %row1_4, align 1
  %d = call i8 @read_past_row_end(ptr %buf)
  %ok4 = icmp eq i8 %d, 17
  call void @__CPROVER_assert(i1 %ok4, ptr null)

  %s = alloca { i32, [4 x i8] }, align 4
  store i32 305419896, ptr %s, align 4
  %e = call i8 @read_struct_field_underflow(ptr %s)
  %ok5 = icmp eq i8 %e, 120
  call void @__CPROVER_assert(i1 %ok5, ptr null)
  ret i32 0
}
