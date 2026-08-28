//! Rust core for the `python-gedcom` package.

mod arena;
mod bounded;
mod element;
mod individual;
mod parser;
mod pystr;
mod scanner;
mod tags;
mod validate;

use pyo3::prelude::*;
use pyo3::types::PyDict;

#[pyfunction]
fn _tag_constants(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
    let constants = PyDict::new(py);
    for (name, value) in tags::ALL {
        constants.set_item(name, value)?;
    }
    Ok(constants)
}

#[pymodule]
fn _gedcom(py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(_tag_constants, module)?)?;
    module.add_class::<arena::Arena>()?;
    module.add_class::<element::Element>()?;
    module.add_class::<individual::IndividualElement>()?;
    module.add_class::<element::FamilyElement>()?;
    module.add_class::<element::FileElement>()?;
    module.add_class::<element::ObjectElement>()?;
    module.add_class::<element::RootElement>()?;
    module.add_class::<parser::Parser>()?;
    module.add_class::<parser::ParseError>()?;
    module.add_class::<parser::SourceInfo>()?;
    module.add_class::<validate::Finding>()?;

    module.add(
        "GedcomFormatViolationError",
        py.get_type::<parser::GedcomFormatViolationError>(),
    )?;
    module.add(
        "NotAnActualIndividualError",
        py.get_type::<individual::NotAnActualIndividualError>(),
    )?;
    module.add(
        "NotAnActualFamilyError",
        py.get_type::<parser::NotAnActualFamilyError>(),
    )?;
    module.add(
        "NotAnActualFileError",
        py.get_type::<element::NotAnActualFileError>(),
    )?;
    module.add(
        "NotAnActualObjectError",
        py.get_type::<element::NotAnActualObjectError>(),
    )?;
    Ok(())
}
