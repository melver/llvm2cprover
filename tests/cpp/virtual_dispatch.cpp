// ---
// description = "virtual dispatch lowers through vtable loads and an indirect call"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern "C" void __CPROVER_assert(bool, const char*);

struct Base {
  virtual int value() const {
    return 1;
  }
};

struct Derived : Base {
  int value() const override {
    return 2;
  }
};

int main() {
  Derived derived;
  Base* base = &derived;
  __CPROVER_assert(base->value() == 2, "virtual dispatch works");
  return 0;
}
