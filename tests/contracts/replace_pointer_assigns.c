// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// expected = "VERIFICATION FAILED"
// description = "pointer-returning assigns helpers havoc the replacement pointee"
// entry = "main"
// translator_flags = ["--replace-with-contract", "target"]
// ---

extern void __CPROVER_assert(_Bool, const char *);

void target(int *value) { *value = 0; }
int *assigns_target(int *value) { return value; }

int main(void) {
    int value = 0;
    target(&value);
    __CPROVER_assert(value == 0, "replacement must havoc the returned pointee");
    return 0;
}
