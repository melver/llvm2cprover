; ---
; description = "Vector saturating and with.overflow intrinsics compute lane-wise"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; esbmc_skip = "ESBMC mis-models CBMC overflow_result expressions"
; ---

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-linux-gnu"
declare i8 @__CPROVER_nondet_char()
declare <2 x i8> @llvm.uadd.sat.v2i8(<2 x i8>, <2 x i8>)
declare <2 x i8> @llvm.usub.sat.v2i8(<2 x i8>, <2 x i8>)
declare <2 x i8> @llvm.sadd.sat.v2i8(<2 x i8>, <2 x i8>)
declare <2 x i8> @llvm.ssub.sat.v2i8(<2 x i8>, <2 x i8>)
declare <2 x i1> @llvm.uadd.sat.v2i1(<2 x i1>, <2 x i1>)
declare <2 x i1> @llvm.usub.sat.v2i1(<2 x i1>, <2 x i1>)
declare <2 x i1> @llvm.sadd.sat.v2i1(<2 x i1>, <2 x i1>)
declare <2 x i1> @llvm.ssub.sat.v2i1(<2 x i1>, <2 x i1>)
declare { <2 x i8>, <2 x i1> } @llvm.uadd.with.overflow.v2i8(<2 x i8>, <2 x i8>)
declare { <2 x i8>, <2 x i1> } @llvm.usub.with.overflow.v2i8(<2 x i8>, <2 x i8>)
declare { <2 x i8>, <2 x i1> } @llvm.umul.with.overflow.v2i8(<2 x i8>, <2 x i8>)
declare { <2 x i8>, <2 x i1> } @llvm.sadd.with.overflow.v2i8(<2 x i8>, <2 x i8>)
declare { <2 x i8>, <2 x i1> } @llvm.ssub.with.overflow.v2i8(<2 x i8>, <2 x i8>)
declare { <2 x i8>, <2 x i1> } @llvm.smul.with.overflow.v2i8(<2 x i8>, <2 x i8>)
declare { <2 x i1>, <2 x i1> } @llvm.uadd.with.overflow.v2i1(<2 x i1>, <2 x i1>)
declare { <2 x i1>, <2 x i1> } @llvm.usub.with.overflow.v2i1(<2 x i1>, <2 x i1>)
declare { <2 x i1>, <2 x i1> } @llvm.umul.with.overflow.v2i1(<2 x i1>, <2 x i1>)
declare { <2 x i1>, <2 x i1> } @llvm.sadd.with.overflow.v2i1(<2 x i1>, <2 x i1>)
declare { <2 x i1>, <2 x i1> } @llvm.ssub.with.overflow.v2i1(<2 x i1>, <2 x i1>)
declare i1 @llvm.vector.reduce.and.v2i1(<2 x i1>)
declare void @__CPROVER_assert(i1, ptr)
define i32 @main() {
entry:
  %uadd = call <2 x i8> @llvm.uadd.sat.v2i8(<2 x i8> <i8 200, i8 100>, <2 x i8> <i8 100, i8 20>)
  %usub = call <2 x i8> @llvm.usub.sat.v2i8(<2 x i8> <i8 3, i8 9>, <2 x i8> <i8 9, i8 3>)
  %sat = call <2 x i8> @llvm.sadd.sat.v2i8(<2 x i8> <i8 120, i8 -120>, <2 x i8> <i8 20, i8 -20>)
  %ssub = call <2 x i8> @llvm.ssub.sat.v2i8(<2 x i8> <i8 -120, i8 120>, <2 x i8> <i8 20, i8 -20>)
  %uao = call { <2 x i8>, <2 x i1> } @llvm.uadd.with.overflow.v2i8(<2 x i8> <i8 200, i8 100>, <2 x i8> <i8 100, i8 20>)
  %uso = call { <2 x i8>, <2 x i1> } @llvm.usub.with.overflow.v2i8(<2 x i8> <i8 3, i8 9>, <2 x i8> <i8 9, i8 3>)
  %uov = call { <2 x i8>, <2 x i1> } @llvm.umul.with.overflow.v2i8(<2 x i8> <i8 16, i8 15>, <2 x i8> <i8 16, i8 17>)
  %sov = call { <2 x i8>, <2 x i1> } @llvm.sadd.with.overflow.v2i8(<2 x i8> <i8 127, i8 1>, <2 x i8> <i8 1, i8 1>)
  %sso = call { <2 x i8>, <2 x i1> } @llvm.ssub.with.overflow.v2i8(<2 x i8> <i8 -128, i8 5>, <2 x i8> <i8 1, i8 2>)
  %smo = call { <2 x i8>, <2 x i1> } @llvm.smul.with.overflow.v2i8(<2 x i8> <i8 20, i8 -5>, <2 x i8> <i8 10, i8 3>)

  %uadd_ok = icmp eq <2 x i8> %uadd, <i8 -1, i8 120>
  %usub_ok = icmp eq <2 x i8> %usub, <i8 0, i8 6>
  %sat_ok = icmp eq <2 x i8> %sat, <i8 127, i8 -128>
  %ssub_ok = icmp eq <2 x i8> %ssub, <i8 -128, i8 127>

  %uav = extractvalue { <2 x i8>, <2 x i1> } %uao, 0
  %uaf = extractvalue { <2 x i8>, <2 x i1> } %uao, 1
  %uav_ok = icmp eq <2 x i8> %uav, <i8 44, i8 120>
  %uaf_ok = icmp eq <2 x i1> %uaf, <i1 true, i1 false>

  %usv = extractvalue { <2 x i8>, <2 x i1> } %uso, 0
  %usf = extractvalue { <2 x i8>, <2 x i1> } %uso, 1
  %usv_ok = icmp eq <2 x i8> %usv, <i8 -6, i8 6>
  %usf_ok = icmp eq <2 x i1> %usf, <i1 true, i1 false>

  %uv = extractvalue { <2 x i8>, <2 x i1> } %uov, 0
  %uf = extractvalue { <2 x i8>, <2 x i1> } %uov, 1
  %uv_ok = icmp eq <2 x i8> %uv, <i8 0, i8 -1>
  %uf_ok = icmp eq <2 x i1> %uf, <i1 true, i1 false>

  %sv = extractvalue { <2 x i8>, <2 x i1> } %sov, 0
  %sf = extractvalue { <2 x i8>, <2 x i1> } %sov, 1
  %sv_ok = icmp eq <2 x i8> %sv, <i8 -128, i8 2>
  %sf_ok = icmp eq <2 x i1> %sf, <i1 true, i1 false>

  %ssv = extractvalue { <2 x i8>, <2 x i1> } %sso, 0
  %ssf = extractvalue { <2 x i8>, <2 x i1> } %sso, 1
  %ssv_ok = icmp eq <2 x i8> %ssv, <i8 127, i8 3>
  %ssf_ok = icmp eq <2 x i1> %ssf, <i1 true, i1 false>

  %smv = extractvalue { <2 x i8>, <2 x i1> } %smo, 0
  %smf = extractvalue { <2 x i8>, <2 x i1> } %smo, 1
  %smv_ok = icmp eq <2 x i8> %smv, <i8 -56, i8 -15>
  %smf_ok = icmp eq <2 x i1> %smf, <i1 true, i1 false>

  %bua = call <2 x i1> @llvm.uadd.sat.v2i1(<2 x i1> <i1 true, i1 false>, <2 x i1> <i1 true, i1 true>)
  %bus = call <2 x i1> @llvm.usub.sat.v2i1(<2 x i1> <i1 false, i1 true>, <2 x i1> <i1 true, i1 false>)
  %bsa = call <2 x i1> @llvm.sadd.sat.v2i1(<2 x i1> <i1 true, i1 false>, <2 x i1> <i1 true, i1 true>)
  %bss = call <2 x i1> @llvm.ssub.sat.v2i1(<2 x i1> <i1 false, i1 true>, <2 x i1> <i1 true, i1 false>)
  %bua_ok = icmp eq <2 x i1> %bua, <i1 true, i1 true>
  %bus_ok = icmp eq <2 x i1> %bus, <i1 false, i1 true>
  %bsa_ok = icmp eq <2 x i1> %bsa, <i1 true, i1 true>
  %bss_ok = icmp eq <2 x i1> %bss, <i1 false, i1 true>

  %buo = call { <2 x i1>, <2 x i1> } @llvm.uadd.with.overflow.v2i1(<2 x i1> <i1 true, i1 false>, <2 x i1> <i1 true, i1 true>)
  %buv = extractvalue { <2 x i1>, <2 x i1> } %buo, 0
  %buf = extractvalue { <2 x i1>, <2 x i1> } %buo, 1
  %buv_ok = icmp eq <2 x i1> %buv, <i1 false, i1 true>
  %buf_ok = icmp eq <2 x i1> %buf, <i1 true, i1 false>

  %buso = call { <2 x i1>, <2 x i1> } @llvm.usub.with.overflow.v2i1(<2 x i1> <i1 false, i1 true>, <2 x i1> <i1 true, i1 true>)
  %busv = extractvalue { <2 x i1>, <2 x i1> } %buso, 0
  %busf = extractvalue { <2 x i1>, <2 x i1> } %buso, 1
  %busv_ok = icmp eq <2 x i1> %busv, <i1 true, i1 false>
  %busf_ok = icmp eq <2 x i1> %busf, <i1 true, i1 false>

  %bumo = call { <2 x i1>, <2 x i1> } @llvm.umul.with.overflow.v2i1(<2 x i1> <i1 true, i1 false>, <2 x i1> <i1 true, i1 true>)
  %bumv = extractvalue { <2 x i1>, <2 x i1> } %bumo, 0
  %bumf = extractvalue { <2 x i1>, <2 x i1> } %bumo, 1
  %bumv_ok = icmp eq <2 x i1> %bumv, <i1 true, i1 false>
  %bumf_ok = icmp eq <2 x i1> %bumf, <i1 false, i1 false>

  %bso = call { <2 x i1>, <2 x i1> } @llvm.sadd.with.overflow.v2i1(<2 x i1> <i1 true, i1 false>, <2 x i1> <i1 true, i1 true>)
  %bsv = extractvalue { <2 x i1>, <2 x i1> } %bso, 0
  %bsf = extractvalue { <2 x i1>, <2 x i1> } %bso, 1
  %bsv_ok = icmp eq <2 x i1> %bsv, <i1 false, i1 true>
  %bsf_ok = icmp eq <2 x i1> %bsf, <i1 true, i1 false>

  %bsso = call { <2 x i1>, <2 x i1> } @llvm.ssub.with.overflow.v2i1(<2 x i1> <i1 false, i1 true>, <2 x i1> <i1 true, i1 true>)
  %bssv = extractvalue { <2 x i1>, <2 x i1> } %bsso, 0
  %bssf = extractvalue { <2 x i1>, <2 x i1> } %bsso, 1
  %bssv_ok = icmp eq <2 x i1> %bssv, <i1 true, i1 false>
  %bssf_ok = icmp eq <2 x i1> %bssf, <i1 true, i1 false>

  %x = call i8 @__CPROVER_nondet_char()
  %vx = insertelement <2 x i8> <i8 0, i8 10>, i8 %x, i64 0
  %sym_sat = call <2 x i8> @llvm.uadd.sat.v2i8(<2 x i8> %vx, <2 x i8> <i8 0, i8 5>)
  %vx_expected = insertelement <2 x i8> <i8 0, i8 15>, i8 %x, i64 0
  %sym_ok = icmp eq <2 x i8> %sym_sat, %vx_expected

  %uadd_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %uadd_ok)
  call void @__CPROVER_assert(i1 %uadd_ok_all, ptr null)
  %usub_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %usub_ok)
  call void @__CPROVER_assert(i1 %usub_ok_all, ptr null)
  %sat_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %sat_ok)
  call void @__CPROVER_assert(i1 %sat_ok_all, ptr null)
  %ssub_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %ssub_ok)
  call void @__CPROVER_assert(i1 %ssub_ok_all, ptr null)
  %uav_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %uav_ok)
  call void @__CPROVER_assert(i1 %uav_ok_all, ptr null)
  %uaf_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %uaf_ok)
  call void @__CPROVER_assert(i1 %uaf_ok_all, ptr null)
  %usv_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %usv_ok)
  call void @__CPROVER_assert(i1 %usv_ok_all, ptr null)
  %usf_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %usf_ok)
  call void @__CPROVER_assert(i1 %usf_ok_all, ptr null)
  %uv_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %uv_ok)
  call void @__CPROVER_assert(i1 %uv_ok_all, ptr null)
  %uf_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %uf_ok)
  call void @__CPROVER_assert(i1 %uf_ok_all, ptr null)
  %sv_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %sv_ok)
  call void @__CPROVER_assert(i1 %sv_ok_all, ptr null)
  %sf_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %sf_ok)
  call void @__CPROVER_assert(i1 %sf_ok_all, ptr null)
  %ssv_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %ssv_ok)
  call void @__CPROVER_assert(i1 %ssv_ok_all, ptr null)
  %ssf_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %ssf_ok)
  call void @__CPROVER_assert(i1 %ssf_ok_all, ptr null)
  %smv_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %smv_ok)
  call void @__CPROVER_assert(i1 %smv_ok_all, ptr null)
  %smf_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %smf_ok)
  call void @__CPROVER_assert(i1 %smf_ok_all, ptr null)
  %bua_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %bua_ok)
  call void @__CPROVER_assert(i1 %bua_ok_all, ptr null)
  %bus_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %bus_ok)
  call void @__CPROVER_assert(i1 %bus_ok_all, ptr null)
  %bsa_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %bsa_ok)
  call void @__CPROVER_assert(i1 %bsa_ok_all, ptr null)
  %bss_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %bss_ok)
  call void @__CPROVER_assert(i1 %bss_ok_all, ptr null)
  %buv_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %buv_ok)
  call void @__CPROVER_assert(i1 %buv_ok_all, ptr null)
  %buf_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %buf_ok)
  call void @__CPROVER_assert(i1 %buf_ok_all, ptr null)
  %busv_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %busv_ok)
  call void @__CPROVER_assert(i1 %busv_ok_all, ptr null)
  %busf_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %busf_ok)
  call void @__CPROVER_assert(i1 %busf_ok_all, ptr null)
  %bumv_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %bumv_ok)
  call void @__CPROVER_assert(i1 %bumv_ok_all, ptr null)
  %bumf_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %bumf_ok)
  call void @__CPROVER_assert(i1 %bumf_ok_all, ptr null)
  %bsv_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %bsv_ok)
  call void @__CPROVER_assert(i1 %bsv_ok_all, ptr null)
  %bsf_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %bsf_ok)
  call void @__CPROVER_assert(i1 %bsf_ok_all, ptr null)
  %bssv_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %bssv_ok)
  call void @__CPROVER_assert(i1 %bssv_ok_all, ptr null)
  %bssf_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %bssf_ok)
  call void @__CPROVER_assert(i1 %bssf_ok_all, ptr null)
  %sym_ok_all = call i1 @llvm.vector.reduce.and.v2i1(<2 x i1> %sym_ok)
  call void @__CPROVER_assert(i1 %sym_ok_all, ptr null)
  ret i32 0
}
