// ---
// description = "A wrapped add is caught by a failing assertion"
// expected = "VERIFICATION FAILED"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  unsigned char x = 255;
  unsigned char y = x + 1;
  __CPROVER_assert(y == 255, "overflow");
  return 0;
}
