// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
//
// Wraps the system csmith.h, replacing its table-driven CRC with an O(1) mixer
// so CBMC's unwinding budget goes to the program under test. Both the native
// oracle (-DFUZZ_NATIVE) and the CBMC build include this header, so the
// checksum comparison stays sound.

#define crc32_gentab          csmith_crc32_gentab_unused
#define crc32_byte            csmith_crc32_byte_unused
#define crc32_8bytes          csmith_crc32_8bytes_unused
#define transparent_crc       csmith_transparent_crc_unused
#define transparent_crc_bytes csmith_transparent_crc_bytes_unused
#include_next "csmith.h"
#undef crc32_gentab
#undef crc32_byte
#undef crc32_8bytes
#undef transparent_crc
#undef transparent_crc_bytes

static void crc32_gentab(void) {}
static void crc32_mix(uint64_t v) {
    crc32_context = (crc32_context ^ (uint32_t)v ^ (uint32_t)(v >> 32)) * 0x9E3779B1u;
    crc32_context ^= crc32_context >> 15;
}
static void transparent_crc(uint64_t val, char *vname, int flag) { (void)vname; (void)flag; crc32_mix(val); }
static void transparent_crc_bytes(char *ptr, int nbytes, char *vname, int flag) {
    (void)vname; (void)flag;
    for (int i = 0; i < nbytes; i++) crc32_mix((unsigned char)ptr[i]);
}

#ifdef FUZZ_NATIVE
static void __CPROVER_assert(int cond, const char *msg) { (void)cond; (void)msg; }
#else
extern void __CPROVER_assert(_Bool cond, const char *msg);
#endif
