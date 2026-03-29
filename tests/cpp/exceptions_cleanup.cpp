// ---
// description = "C++ EH cleanup paths lower through cleanup landingpads and resume"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern "C" void __CPROVER_assert(bool, const char*);

struct Guard {
  ~Guard();
};

Guard::~Guard() {}

int may_throw(int x) {
  if (x == 0) {
    throw 1;
  }
  return x;
}

int wrapper() {
  Guard guard;
  return may_throw(0);
}

int main() {
  int result = -1;
  try {
    result = wrapper();
  } catch (...) {
    result = 0;
  }
  __CPROVER_assert(result == 0, "cleanup path resumes into outer catch");
  return 0;
}
