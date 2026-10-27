; ---
; description = "vector getelementptr fails closed instead of crashing"
; expected = "TRANSLATION FAILED"
; entry = "main"
; ---

@a = global [4 x i32] zeroinitializer, align 16

define i32 @main() {
entry:
  %v = getelementptr i32, <2 x ptr> <ptr @a, ptr @a>, <2 x i64> <i64 0, i64 1>
  %p = extractelement <2 x ptr> %v, i64 1
  %x = load i32, ptr %p, align 4
  ret i32 %x
}
