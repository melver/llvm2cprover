// ---
// description = "simple Rust methods on structs lower through direct calls"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "harness"
// ---

extern "C" {
    fn __CPROVER_assert(cond: bool, descr: *const i8);
}

struct Counter {
    base: i32,
}

impl Counter {
    fn next(&self) -> i32 {
        self.base + 1
    }
}

#[no_mangle]
pub unsafe extern "C" fn harness() -> i32 {
    let counter = Counter { base: 41 };
    __CPROVER_assert(counter.next() == 42, b"rust method works\0".as_ptr().cast());
    0
}
