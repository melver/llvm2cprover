; ---
; description = "named alloca struct keeps ABI padding when first seen during pointer-hint collection"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"

%struct.info = type { i64, i64, i32, i32, i64, i32, i32, i32, i32, i32, i32, i32, i32, i32, i64 }

define i32 @main() {
entry:
  %obj = alloca %struct.info, align 8
  %field = getelementptr i8, ptr %obj, i64 72
  store i64 0, ptr %field, align 8
  ret i32 0
}
