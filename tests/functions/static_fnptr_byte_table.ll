; ---
; description = "Indirect call through a zeroed byte-array table of function pointers calls only the stored target"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; cbmc_flags = ["--unwind", "9", "--unwinding-assertions"]
; ---
; rustc emits a zeroed `static mut T: [Option<fn(i32) -> i32>; 13]` as
; `[104 x i8]`. Accessed only as pointers at pointer-aligned offsets, it is typed
; as `[13 x ptr]`: as one 104-byte array (above CBMC's default
; --max-field-sensitivity-array-size) the slot's value set would hold every
; pointer stored in the table, and symex would enter all 13 functions.

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-unknown-linux-gnu"

@table = internal global [104 x i8] zeroinitializer, align 8
declare i32 @__CPROVER_nondet_int()
declare void @__CPROVER_assert(i1, ptr)

define internal i32 @f0(i32 %x) {
entry:
  br label %loop
loop:
  %i = phi i32 [ 0, %entry ], [ %n, %loop ]
  %a = phi i32 [ %x, %entry ], [ %b, %loop ]
  %b = mul i32 %a, 3
  %n = add i32 %i, 1
  %c = icmp ult i32 %n, 8
  br i1 %c, label %loop, label %exit
exit:
  ret i32 %b
}

define internal i32 @f1(i32 %x) {
entry:
  br label %loop
loop:
  %i = phi i32 [ 0, %entry ], [ %n, %loop ]
  %a = phi i32 [ %x, %entry ], [ %b, %loop ]
  %b = mul i32 %a, 4
  %n = add i32 %i, 1
  %c = icmp ult i32 %n, 8
  br i1 %c, label %loop, label %exit
exit:
  ret i32 %b
}

define internal i32 @f2(i32 %x) {
entry:
  br label %loop
loop:
  %i = phi i32 [ 0, %entry ], [ %n, %loop ]
  %a = phi i32 [ %x, %entry ], [ %b, %loop ]
  %b = mul i32 %a, 5
  %n = add i32 %i, 1
  %c = icmp ult i32 %n, 8
  br i1 %c, label %loop, label %exit
exit:
  ret i32 %b
}

define internal i32 @f3(i32 %x) {
entry:
  br label %loop
loop:
  %i = phi i32 [ 0, %entry ], [ %n, %loop ]
  %a = phi i32 [ %x, %entry ], [ %b, %loop ]
  %b = mul i32 %a, 6
  %n = add i32 %i, 1
  %c = icmp ult i32 %n, 8
  br i1 %c, label %loop, label %exit
exit:
  ret i32 %b
}

define internal i32 @f4(i32 %x) {
entry:
  br label %loop
loop:
  %i = phi i32 [ 0, %entry ], [ %n, %loop ]
  %a = phi i32 [ %x, %entry ], [ %b, %loop ]
  %b = mul i32 %a, 7
  %n = add i32 %i, 1
  %c = icmp ult i32 %n, 8
  br i1 %c, label %loop, label %exit
exit:
  ret i32 %b
}

define internal i32 @f5(i32 %x) {
entry:
  br label %loop
loop:
  %i = phi i32 [ 0, %entry ], [ %n, %loop ]
  %a = phi i32 [ %x, %entry ], [ %b, %loop ]
  %b = mul i32 %a, 8
  %n = add i32 %i, 1
  %c = icmp ult i32 %n, 8
  br i1 %c, label %loop, label %exit
exit:
  ret i32 %b
}

define internal i32 @f6(i32 %x) {
entry:
  br label %loop
loop:
  %i = phi i32 [ 0, %entry ], [ %n, %loop ]
  %a = phi i32 [ %x, %entry ], [ %b, %loop ]
  %b = mul i32 %a, 9
  %n = add i32 %i, 1
  %c = icmp ult i32 %n, 8
  br i1 %c, label %loop, label %exit
exit:
  ret i32 %b
}

define internal i32 @f7(i32 %x) {
entry:
  br label %loop
loop:
  %i = phi i32 [ 0, %entry ], [ %n, %loop ]
  %a = phi i32 [ %x, %entry ], [ %b, %loop ]
  %b = mul i32 %a, 10
  %n = add i32 %i, 1
  %c = icmp ult i32 %n, 8
  br i1 %c, label %loop, label %exit
exit:
  ret i32 %b
}

define internal i32 @f8(i32 %x) {
entry:
  br label %loop
loop:
  %i = phi i32 [ 0, %entry ], [ %n, %loop ]
  %a = phi i32 [ %x, %entry ], [ %b, %loop ]
  %b = mul i32 %a, 11
  %n = add i32 %i, 1
  %c = icmp ult i32 %n, 8
  br i1 %c, label %loop, label %exit
exit:
  ret i32 %b
}

define internal i32 @f9(i32 %x) {
entry:
  br label %loop
loop:
  %i = phi i32 [ 0, %entry ], [ %n, %loop ]
  %a = phi i32 [ %x, %entry ], [ %b, %loop ]
  %b = mul i32 %a, 12
  %n = add i32 %i, 1
  %c = icmp ult i32 %n, 8
  br i1 %c, label %loop, label %exit
exit:
  ret i32 %b
}

define internal i32 @f10(i32 %x) {
entry:
  br label %loop
loop:
  %i = phi i32 [ 0, %entry ], [ %n, %loop ]
  %a = phi i32 [ %x, %entry ], [ %b, %loop ]
  %b = mul i32 %a, 13
  %n = add i32 %i, 1
  %c = icmp ult i32 %n, 8
  br i1 %c, label %loop, label %exit
exit:
  ret i32 %b
}

define internal i32 @f11(i32 %x) {
entry:
  br label %loop
loop:
  %i = phi i32 [ 0, %entry ], [ %n, %loop ]
  %a = phi i32 [ %x, %entry ], [ %b, %loop ]
  %b = mul i32 %a, 14
  %n = add i32 %i, 1
  %c = icmp ult i32 %n, 8
  br i1 %c, label %loop, label %exit
exit:
  ret i32 %b
}

define internal i32 @f12(i32 %x) {
entry:
  br label %loop
loop:
  %i = phi i32 [ 0, %entry ], [ %n, %loop ]
  %a = phi i32 [ %x, %entry ], [ %b, %loop ]
  %b = mul i32 %a, 15
  %n = add i32 %i, 1
  %c = icmp ult i32 %n, 8
  br i1 %c, label %loop, label %exit
exit:
  ret i32 %b
}

define i32 @main() {
entry:
  store ptr @f0, ptr getelementptr inbounds nuw (i8, ptr @table, i64 0), align 8
  store ptr @f1, ptr getelementptr inbounds nuw (i8, ptr @table, i64 8), align 8
  store ptr @f2, ptr getelementptr inbounds nuw (i8, ptr @table, i64 16), align 8
  store ptr @f3, ptr getelementptr inbounds nuw (i8, ptr @table, i64 24), align 8
  store ptr @f4, ptr getelementptr inbounds nuw (i8, ptr @table, i64 32), align 8
  store ptr @f5, ptr getelementptr inbounds nuw (i8, ptr @table, i64 40), align 8
  store ptr @f6, ptr getelementptr inbounds nuw (i8, ptr @table, i64 48), align 8
  store ptr @f7, ptr getelementptr inbounds nuw (i8, ptr @table, i64 56), align 8
  store ptr @f8, ptr getelementptr inbounds nuw (i8, ptr @table, i64 64), align 8
  store ptr @f9, ptr getelementptr inbounds nuw (i8, ptr @table, i64 72), align 8
  store ptr @f10, ptr getelementptr inbounds nuw (i8, ptr @table, i64 80), align 8
  store ptr @f11, ptr getelementptr inbounds nuw (i8, ptr @table, i64 88), align 8
  store ptr @f12, ptr getelementptr inbounds nuw (i8, ptr @table, i64 96), align 8
  %slot = getelementptr inbounds nuw [8 x i8], ptr @table, i64 5
  %fp = load ptr, ptr %slot, align 8
  %x = call i32 @__CPROVER_nondet_int()
  %r = call i32 %fp(i32 %x)
  %e = call i32 @f5(i32 %x)
  %ok = icmp eq i32 %r, %e
  call void @__CPROVER_assert(i1 %ok, ptr null)
  ret i32 0
}
