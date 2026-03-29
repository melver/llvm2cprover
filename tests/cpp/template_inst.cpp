// ---
// description = "simple template instantiations lower as ordinary direct calls"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern "C" void __CPROVER_assert(bool, const char*);

template <typename T>
T add_one(T x) {
  return x + 1;
}

int main() {
  __CPROVER_assert(add_one<int>(4) == 5, "template call works");
  return 0;
}
