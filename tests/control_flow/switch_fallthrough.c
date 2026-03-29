// ---
// description = "Switch fallthrough stays faithful to C control-flow shape"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

static int f(int x) {
  int out = 0;
  switch (x) {
  case 0:
    out += 1;
  case 1:
    out += 2;
    break;
  default:
    out += 4;
    break;
  }
  return out;
}

int main(void) {
  __CPROVER_assert(f(0) == 3, "fallthrough");
  return 0;
}
