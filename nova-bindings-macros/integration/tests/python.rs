//! Drives the generated PyO3 classes from Python code.
#![cfg(feature = "python")]

use std::ffi::CString;

use nova_bindings_integration::python::fixtures;
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Runs `code` with the fixture module imported as `fx`.
fn run(code: &str) {
    Python::attach(|py| {
        let module = PyModule::new(py, "fixtures").unwrap();
        fixtures(&module).unwrap();
        let globals = PyDict::new(py);
        globals.set_item("fx", module).unwrap();
        let code = CString::new(code).unwrap();
        if let Err(error) = py.run(&code, Some(&globals), None) {
            error.print(py);
            panic!("Python code failed");
        }
    });
}

#[test]
fn constructor_preserves_names_and_types() {
    run(r#"
import inspect
assert fx.Ema.__name__ == "Ema"
assert str(inspect.signature(fx.Ema)) == "(period, smoothing)"
assert "Exponential moving average." in fx.Ema.__doc__
ema = fx.Ema(period=3, smoothing=2.0)
assert ema.samples() == 0 and ema.last() is None
"#);
}

#[test]
fn update_override_takes_batches() {
    run(r#"
ema = fx.Ema(3, 2.0)
out = ema.update([1.0, 2.0, 3.0])
assert out == [1.0, 1.5, 2.25], out
assert ema.ready() and ema.samples() == 3 and ema.last() == 2.25
assert "batch" in fx.Ema.update.__doc__
"#);
}

#[test]
fn callable_policy_applies_to_the_override() {
    run(r#"
ema = fx.Ema(3, 2.0)
assert ema([4.0]) == [4.0]
assert ema([6.0]) == [5.0] and ema.last() == 5.0
"#);
}

#[test]
fn reset_restarts_warmup() {
    run(r#"
ema = fx.Ema(2, 2.0)
ema.update([1.0, 2.0])
assert ema.ready()
assert ema.reset() is None
assert not ema.ready() and ema.samples() == 0 and ema.last() is None
"#);
}

#[test]
fn errors_map_to_the_configured_exception() {
    run(r#"
assert issubclass(fx.FixtureError, RuntimeError)
try:
    fx.Ema(0, 2.0)
    raise AssertionError("expected FixtureError")
except fx.FixtureError as error:
    assert str(error) == "period must be at least 1"
ema = fx.Ema(3, 2.0)
try:
    ema.update([1.0, float("nan")])
    raise AssertionError("expected FixtureError")
except fx.FixtureError as error:
    assert "input must be finite" in str(error)
"#);
}

#[test]
fn explicit_constructor_and_cross_module_impls() {
    run(r#"
import inspect
counter = fx.Counter(1.5)
assert str(inspect.signature(fx.Counter)) == "(threshold)"
assert counter.threshold() == 1.5
assert [counter.update(x) for x in [1.0, 2.0, 3.0]] == [0.0, 1.0, 2.0]
assert not hasattr(counter, "__call__")
counter.reset()
assert counter.update(5.0) == 1.0 and counter.ready()
"#);
}
