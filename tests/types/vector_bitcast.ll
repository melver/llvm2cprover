; ---
; description = "bitcasts to integer vectors repack the bits: vector-to-vector across lane widths and scalar-to-vector"
; expected = "VERIFICATION SUCCESSFUL"
; entry = "main"
; ---
;
; Csmith seed 43844 (-O2) reads bit 16 of a <2 x i16> as
; `extractelement (bitcast <2 x i16> %v to <32 x i1>), 16`.

target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-linux-gnu"

declare void @__CPROVER_assert(i1, ptr)
declare i16 @__CPROVER_nondet_ushort()

@.bits = private unnamed_addr constant [5 x i8] c"bits\00"
@.bytes = private unnamed_addr constant [6 x i8] c"bytes\00"
@.halves = private unnamed_addr constant [7 x i8] c"halves\00"

define i32 @main() {
entry:
  ; <2 x i16> -> <32 x i1>: lane 16 is bit 0 of the second i16 (little endian).
  %x = call i16 @__CPROVER_nondet_ushort()
  %v = insertelement <2 x i16> <i16 0, i16 0>, i16 %x, i64 1
  %b = bitcast <2 x i16> %v to <32 x i1>
  %b16 = extractelement <32 x i1> %b, i64 16
  %b0 = extractelement <32 x i1> %b, i64 0
  %x0 = trunc i16 %x to i1
  %ok.bits.1 = icmp eq i1 %b16, %x0
  %ok.bits.2 = xor i1 %b0, true
  %ok.bits = and i1 %ok.bits.1, %ok.bits.2
  call void @__CPROVER_assert(i1 %ok.bits, ptr @.bits)

  ; i32 -> <4 x i8>: lane 0 is the low byte.
  %w = bitcast i32 305419896 to <4 x i8>   ; 0x12345678
  %w0 = extractelement <4 x i8> %w, i64 0
  %w3 = extractelement <4 x i8> %w, i64 3
  %ok.bytes.0 = icmp eq i8 %w0, 120         ; 0x78
  %ok.bytes.3 = icmp eq i8 %w3, 18          ; 0x12
  %ok.bytes = and i1 %ok.bytes.0, %ok.bytes.3
  call void @__CPROVER_assert(i1 %ok.bytes, ptr @.bytes)

  ; <4 x i8> -> <2 x i16>: bytes pair up little-endian.
  %h = bitcast <4 x i8> <i8 1, i8 2, i8 3, i8 4> to <2 x i16>
  %h0 = extractelement <2 x i16> %h, i64 0
  %h1 = extractelement <2 x i16> %h, i64 1
  %ok.halves.0 = icmp eq i16 %h0, 513       ; 0x0201
  %ok.halves.1 = icmp eq i16 %h1, 1027      ; 0x0403
  %ok.halves = and i1 %ok.halves.0, %ok.halves.1
  call void @__CPROVER_assert(i1 %ok.halves, ptr @.halves)
  ret i32 0
}
