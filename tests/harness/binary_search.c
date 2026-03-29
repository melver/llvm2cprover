// ---
// description = "A bounded binary search harness verifies a successful lookup"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// cbmc_flags = ["--unwind", "6"]
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

static int binary_search(const int* xs, int len, int target) {
  int low = 0;
  int high = len - 1;
  while (low <= high) {
    int mid = low + (high - low) / 2;
    if (xs[mid] == target) {
      return mid;
    }
    if (xs[mid] < target) {
      low = mid + 1;
    } else {
      high = mid - 1;
    }
  }
  return -1;
}

int main(void) {
  int xs[5] = {1, 3, 5, 7, 9};
  __CPROVER_assert(binary_search(xs, 5, 7) == 3, "binary search");
  return 0;
}
