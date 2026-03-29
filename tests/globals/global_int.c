// ---
// description = "Global integer initialization and mutation work"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int g = 3;

int main(void) {
  g = g + 2;
  __CPROVER_assert(g == 5, "global");
  return 0;
}
