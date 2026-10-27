// ---
// description = "8-byte store straddling an unaligned packed struct member (CBMC byte_update bug)"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// todo = "known CBMC bug (6.6.0, 6.8.0, 6.11.0): the store loses the bytes it shares with the unaligned member"
// ---
//
// Not a translator bug: `cbmc` on this file alone fails the same way. The store at
// offset 8 straddles member `c` (bytes 4..11), and CBMC drops bytes 8..11 of the
// update; bytes 12..15 survive.
//
// Found by Csmith seed 8120, where Clang returns a bit-field struct by value as
// `{ i64, i64 }` and stores the second half into an alloca of the packed
// `<{ i16, [2 x i8], i64, i32 }>`. Drop the `todo` once a CBMC release fixes it.

extern void __CPROVER_assert(_Bool cond, const char *message);

struct __attribute__((packed)) S {
  unsigned short a;
  unsigned char b[2];
  unsigned long c;
  unsigned int d;
};

int main(void) {
  struct S s;
  *(unsigned long *)((char *)&s + 8) = 0x072000D200007FDAul;
  __CPROVER_assert(((unsigned char *)&s)[8] == 0xDA, "byte shared with c");
  __CPROVER_assert(((unsigned char *)&s)[12] == 0xD2, "byte past c");
  return 0;
}
