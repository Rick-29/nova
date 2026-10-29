//! Writes the `nova_ta.pyi` type stubs: `cargo run --bin stub_gen --features python-stubs`.

fn main() -> pyo3_stub_gen::Result<()> {
    nova_ta::python::stub_info()?.generate()?;
    Ok(())
}
