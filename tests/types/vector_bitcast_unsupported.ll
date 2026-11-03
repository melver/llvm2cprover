; ---
; description = "constant-expression vector bitcasts fail closed without panicking"
; expected = "TRANSLATION FAILED"
; entry = "main"
; ---

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-linux-gnu"

@g = global i32 0
@c = global <8 x i8> bitcast (i64 ptrtoint (ptr @g to i64) to <8 x i8>)

define i32 @main() {
entry:
  ret i32 0
}
