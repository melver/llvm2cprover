// ---
// description = "Bitwise and/or/xor lower directly to integer operations"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  unsigned x = 0x6u;
  unsigned y = 0xAu;
  __CPROVER_assert((x & y) == 0x2u, "and");
  __CPROVER_assert((x | y) == 0xEu, "or");
  __CPROVER_assert((x ^ y) == 0xCu, "xor");
  return 0;
}
