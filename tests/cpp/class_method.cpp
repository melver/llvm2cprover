// ---
// description = "simple non-virtual member calls lower as direct calls"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern "C" void __CPROVER_assert(bool, const char*);

struct Counter {
  int base;

  int next() const {
    return base + 1;
  }
};

int main() {
  Counter c{41};
  __CPROVER_assert(c.next() == 42, "member call works");
  return 0;
}
