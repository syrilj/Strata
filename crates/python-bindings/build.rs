//! Link as a Python extension module.
//!
//! On macOS a Python extension must be linked with `-undefined dynamic_lookup`,
//! because the `Py_*` symbols are provided by the interpreter that loads it.
//! Since PyO3 0.26 this is no longer done implicitly for plain `cargo build`,
//! so CI's macOS `cargo clippy/build --all-targets` failed at link time.
fn main() {
    pyo3_build_config::add_extension_module_link_args();
}
