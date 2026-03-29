; ---
; description = "llvm.trap becomes a verification failure"
; expected = "VERIFICATION FAILED"
; entry = "main"
; ---

declare void @llvm.trap()

define i32 @main() {
entry:
  call void @llvm.trap()
  ret i32 0
}
