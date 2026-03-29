// ---
// description = "C++ exceptions lower through invoke, landingpad, and catch-all handling"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern "C" void __CPROVER_assert(bool, const char*);

int may_throw(int x) {
  if (x == 0) {
    throw 1;
  }
  return x;
}

int main() {
  int result = -1;
  try {
    result = may_throw(0);
  } catch (...) {
    result = 0;
  }
  __CPROVER_assert(result == 0, "catch-all handled thrown exception");
  return 0;
}
