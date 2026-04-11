; ---
; description = "undef global initialisers are zero (static-storage zero-init)"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---

@g_int    = global i32 undef
@g_float  = global float undef
@g_array  = global [3 x i32] undef

@.msg = private unnamed_addr constant [13 x i8] c"global undef\00"

declare void @__CPROVER_assert(i1, ptr)

define i32 @main() {
entry:
  %vi  = load i32,   ptr @g_int
  %vf  = load float, ptr @g_float
  %p0  = getelementptr [3 x i32], ptr @g_array, i32 0, i32 0
  %v0  = load i32, ptr %p0

  %ok1 = icmp eq i32   %vi, 0
  %ok2 = fcmp oeq float %vf, 0.0
  %ok3 = icmp eq i32   %v0, 0
  %ok12  = and i1 %ok1, %ok2
  %ok    = and i1 %ok12, %ok3
  call void @__CPROVER_assert(i1 %ok, ptr @.msg)
  ret i32 0
}
