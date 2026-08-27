//! Python string semantics that Rust's standard library does not match.

pub fn is_python_space(character: char) -> bool {
    character.is_whitespace()
        || matches!(character, '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{1f}')
}

pub fn strip(text: &str) -> &str {
    text.trim_matches(is_python_space)
}

pub fn last_whitespace_token(text: &str) -> Option<&str> {
    text.split(is_python_space).filter(|part| !part.is_empty()).next_back()
}

pub fn drop_first_char(text: &str) -> &str {
    let mut characters = text.chars();
    characters.next();
    characters.as_str()
}

pub fn rstrip_spaces(text: &str) -> &str {
    text.trim_end_matches(' ')
}

pub fn python_int(
    py: pyo3::Python<'_>,
    text: &str,
) -> pyo3::PyResult<Option<pyo3::Py<pyo3::PyAny>>> {
    use pyo3::prelude::*;

    if let Some(value) = fast_int(text) {
        return Ok(Some(value.into_pyobject(py)?.into_any().unbind()));
    }

    match py.import("builtins")?.call_method1("int", (text,)) {
        Ok(value) => Ok(Some(value.unbind())),
        Err(error) if error.is_instance_of::<pyo3::exceptions::PyValueError>(py) => Ok(None),
        Err(error) => Err(error),
    }
}

fn fast_int(text: &str) -> Option<i64> {
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    if digits.is_empty() || digits.len() > 18 {
        return None;
    }
    if !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse::<i64>().ok()
}
