// ---
// description = "Mixed-width integer values can coexist in one translated function"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  unsigned char a = 1;
  unsigned short b = 2;
  unsigned c = 3;
  unsigned long long d = 4;
  __CPROVER_assert((unsigned long long)a + b + c + d == 10, "widths");
  return 0;
}
