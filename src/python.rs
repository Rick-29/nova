//! The `nova_ta` Python extension module.
//!
//! Build it with `maturin develop --features python`. Classes are generated
//! by `nova-bindings-macros` next to each annotated indicator.

use pyo3::prelude::*;

pub use crate::error::PyNovaError;
use crate::filters::alma::PyAlma;
use crate::filters::kama::PyKama;
use crate::filters::lsma::PyLsma;
pub use crate::filters::ultimate_smoother::PyUltimateSmoother;
use crate::filters::wma::PyLinearWma;
pub use crate::statistics::cti::PyCti;
use crate::structure::ker::PyKer;

#[pymodule]
fn nova_ta(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Filters
    m.add_class::<PyAlma>()?;
    m.add_class::<PyKama>()?;
    m.add_class::<PyLsma>()?;
    m.add_class::<PyUltimateSmoother>()?;
    m.add_class::<PyLinearWma>()?;

    // Statistics
    m.add_class::<PyCti>()?;

    // Structure
    m.add_class::<PyKer>()?;

    // Volatility

    m.add("NovaError", m.py().get_type::<PyNovaError>())?;
    Ok(())
}

#[cfg(feature = "python-stubs")]
pyo3_stub_gen::define_stub_info_gatherer!(stub_info);
