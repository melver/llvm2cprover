// ---
// description = "Rust division by zero lowers core::panicking::panic_const::panic_const_div_by_zero to a failure"
// expected = "VERIFICATION FAILED"
// entry = "harness"
// ---

fn divide(lhs: i32, rhs: i32) -> i32 {
    lhs / rhs
}

#[no_mangle]
pub extern "C" fn harness() -> i32 {
    divide(10, 0)
}
