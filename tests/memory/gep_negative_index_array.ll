; ---
; description = "negative byte GEP on a subarray must address the preceding element rather than indexing the subarray value out of bounds"
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

define i32 @main() {
entry:
  %buf = alloca [2 x [16 x i8]], align 16
  %target = getelementptr inbounds [2 x [16 x i8]], ptr %buf, i64 0, i64 0, i64 12
  store i8 42, ptr %target, align 4
  %a = call i8 @read_via_decayed_base(ptr %buf, i64 1)
  %b = call i8 @read_via_two_index_gep(ptr %buf, i64 1)
  %ok1 = icmp eq i8 %a, 42
  %ok2 = icmp eq i8 %b, 42
  call void @__CPROVER_assert(i1 %ok1, ptr null)
  call void @__CPROVER_assert(i1 %ok2, ptr null)
  call void @write_via_two_index_gep(ptr %buf, i64 1, i8 99)
  %c = load i8, ptr %target, align 4
  %ok3 = icmp eq i8 %c, 99
  call void @__CPROVER_assert(i1 %ok3, ptr null)
  ret i32 0
}
