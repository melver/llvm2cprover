; ---
; description = "Byte GEP into a large struct global does not produce a static bounds-check failure"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-linux-gnu"

%struct.fd_vm = type { ptr, i64, i64, ptr, i64, ptr, i64, i64, i64, i64, ptr, ptr, ptr, i64, i64, i64, i64, i64, [6 x i64], [6 x i32], [6 x i32], ptr, i32, ptr, i8, [16 x i64], [64 x %struct.fd_vm_shadow], [262144 x i8], [262144 x i8], ptr, i64, i32, i32, i32, i64, i64, i64, i64, i8, i64, i32, [12 x i8] }
%struct.fd_vm_shadow = type { i64, i64, i64, i64, i64, i64 }

@CPROVER_harness.vm = internal global [1 x %struct.fd_vm] zeroinitializer

define void @main() {
  store i64 0, ptr getelementptr inbounds nuw (i8, ptr @CPROVER_harness.vm, i64 527832), align 8
  ret void
}
