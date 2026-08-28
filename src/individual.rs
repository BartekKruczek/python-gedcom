//! `IndividualElement` -- the accessors for a single person's record.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyList;
use pyo3::types::PyTuple;

use crate::arena::Kind;
use crate::arena::ArenaData;
use crate::element::{children_of, with_children, Element};
use crate::pystr;
use crate::tags;

pyo3::create_exception!(
    gedcom.element.individual,
    NotAnActualIndividualError,
    pyo3::exceptions::PyException
);

/// GEDCOM element consisting of tag `INDI`.
#[pyclass(extends = Element, subclass, module = "gedcom.element.individual")]
pub struct IndividualElement;

fn event_data(
    py: Python<'_>,
    object: &Bound<'_, PyAny>,
    tag: &str,
) -> PyResult<(String, String, Vec<String>)> {
    with_children(
        py,
        object,
        |data, id| Ok(native_event_data(data, id, tag)),
        |children| {
            let mut date = String::new();
            let mut place = String::new();
            let mut sources: Vec<String> = Vec::new();

            for child in children {
                if !child.tag_matches(py, tag)? {
                    continue;
                }
                for grandchild in child.children(py)? {
                    let grandchild_tag = grandchild.tag(py)?;
                    if grandchild_tag == tags::DATE {
                        date = grandchild.value_str(py)?;
                    }
                    if grandchild_tag == tags::PLACE {
                        place = grandchild.value_str(py)?;
                    }
                    if grandchild_tag == tags::SOURCE {
                        sources.push(grandchild.value_str(py)?);
                    }
                }
            }

            Ok((date, place, sources))
        },
    )
}

fn native_event_data(
    data: &ArenaData,
    id: u32,
    tag: &str,
) -> Option<(String, String, Vec<String>)> {
    let mut date = String::new();
    let mut place = String::new();
    let mut sources: Vec<String> = Vec::new();

    for child_id in data.children_of(id)? {
        if data.effective_tag_of(child_id) != tag {
            continue;
        }
        for grandchild_id in data.children_of(child_id)? {
            match data.effective_tag_of(grandchild_id) {
                tags::DATE => date = data.value_of(grandchild_id).to_owned(),
                tags::PLACE => place = data.value_of(grandchild_id).to_owned(),
                tags::SOURCE => sources.push(data.value_of(grandchild_id).to_owned()),
                _ => {}
            }
        }
    }

    Some((date, place, sources))
}

fn event_year(py: Python<'_>, object: &Bound<'_, PyAny>, tag: &str) -> PyResult<Py<PyAny>> {
    let date: Option<String> = with_children(
        py,
        object,
        |data, id| native_event_date(data, id, tag).transpose(),
        |children| {
            let mut date: Option<String> = None;
            for child in children {
                if !child.tag_matches(py, tag)? {
                    continue;
                }
                for grandchild in child.children(py)? {
                    if grandchild.tag(py)? != tags::DATE {
                        continue;
                    }
                    let value = grandchild.value_str(py)?;
                    date = Some(last_token_or_raise(&value)?.to_owned());
                }
            }
            Ok(date)
        },
    )?;

    let Some(date) = date else {
        return Ok((-1i64).into_pyobject(py)?.into_any().unbind());
    };
    if date.is_empty() {
        return Ok((-1i64).into_pyobject(py)?.into_any().unbind());
    }

    parse_int_or_minus_one(py, &date)
}

fn last_token_or_raise(value: &str) -> PyResult<&str> {
    pystr::last_whitespace_token(value)
        .ok_or_else(|| pyo3::exceptions::PyIndexError::new_err("list index out of range"))
}

fn native_event_date(data: &ArenaData, id: u32, tag: &str) -> Option<PyResult<Option<String>>> {
    let mut date: Option<String> = None;

    for child_id in data.children_of(id)? {
        if data.effective_tag_of(child_id) != tag {
            continue;
        }
        for grandchild_id in data.children_of(child_id)? {
            if data.effective_tag_of(grandchild_id) != tags::DATE {
                continue;
            }
            match last_token_or_raise(data.value_of(grandchild_id)) {
                Ok(token) => date = Some(token.to_owned()),
                Err(error) => return Some(Err(error)),
            }
        }
    }

    Some(Ok(date))
}

fn parse_int_or_minus_one(py: Python<'_>, text: &str) -> PyResult<Py<PyAny>> {
    match pystr::python_int(py, text)? {
        Some(value) => Ok(value),
        None => Ok((-1i64).into_pyobject(py)?.into_any().unbind()),
    }
}


fn is_plain(slf: &Bound<'_, IndividualElement>) -> bool {
    slf.as_any().is_exact_instance_of::<IndividualElement>()
}

fn name_of(slf: &Bound<'_, IndividualElement>) -> PyResult<Py<PyAny>> {
    if is_plain(slf) {
        return IndividualElement::get_name(slf);
    }
    Ok(slf.as_any().call_method0("get_name")?.unbind())
}

fn event_year_of(slf: &Bound<'_, IndividualElement>, birth: bool) -> PyResult<Py<PyAny>> {
    if is_plain(slf) {
        return event_year(
            slf.py(),
            slf.as_any(),
            if birth { tags::BIRTH } else { tags::DEATH },
        );
    }
    let name = if birth {
        "get_birth_year"
    } else {
        "get_death_year"
    };
    Ok(slf.as_any().call_method0(name)?.unbind())
}

fn regex_search(py: Python<'_>, pattern: &Bound<'_, PyAny>, text: &str) -> PyResult<Py<PyAny>> {
    static SEARCH: pyo3::sync::PyOnceLock<(Py<PyAny>, Py<PyAny>)> =
        pyo3::sync::PyOnceLock::new();

    let (search, ignore_case) = SEARCH.get_or_try_init(py, || -> PyResult<_> {
        let module = py.import("re")?;
        Ok((
            module.getattr("search")?.unbind(),
            module.getattr("IGNORECASE")?.unbind(),
        ))
    })?;

    Ok(search
        .bind(py)
        .call1((pattern, text, ignore_case.bind(py)))?
        .unbind())
}

fn to_tuple(py: Python<'_>, first: &str, second: &str) -> PyResult<Py<PyAny>> {
    Ok(PyTuple::new(py, [first, second])?.into_any().unbind())
}

#[pymethods]
impl IndividualElement {
    #[new]
    #[pyo3(signature = (level, pointer, tag, value, crlf = "\n", multi_line = true))]
    fn py_new(
        py: Python<'_>,
        level: i64,
        pointer: Option<String>,
        tag: String,
        value: String,
        crlf: &str,
        multi_line: bool,
    ) -> PyResult<PyClassInitializer<Self>> {
        let element = Element::create(
            py,
            level,
            pointer,
            tag,
            value.clone(),
            crlf.to_owned(),
            Kind::Individual,
        )?;
        if multi_line {
            element.apply_multi_line_value(py, None, &value)?;
        }
        Ok(PyClassInitializer::from(element).add_subclass(IndividualElement))
    }

    fn get_tag(&self) -> &'static str {
        tags::INDIVIDUAL
    }

    /// Checks if this individual is deceased
    /// :rtype: bool
    fn is_deceased(slf: &Bound<'_, Self>) -> PyResult<bool> {
        let py = slf.py();
        with_children(
            py,
            slf.as_any(),
            |data, id| {
                Ok(data
                    .children_of(id)
                    .map(|mut ids| ids.any(|c| data.effective_tag_of(c) == tags::DEATH)))
            },
            |children| {
                for child in children {
                    if child.tag_matches(py, tags::DEATH)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            },
        )
    }

    /// Checks if this element is a child of a family
    /// :rtype: bool
    fn is_child(slf: &Bound<'_, Self>) -> PyResult<bool> {
        let py = slf.py();
        with_children(
            py,
            slf.as_any(),
            |data, id| {
                Ok(data
                    .children_of(id)
                    .map(|mut ids| ids.any(|c| data.effective_tag_of(c) == tags::FAMILY_CHILD)))
            },
            |children| {
                let mut found = false;
                for child in children {
                    if child.tag_matches(py, tags::FAMILY_CHILD)? {
                        found = true;
                    }
                }
                Ok(found)
            },
        )
    }

    /// Checks if this individual is marked private
    /// :rtype: bool
    fn is_private(slf: &Bound<'_, Self>) -> PyResult<bool> {
        let py = slf.py();
        with_children(
            py,
            slf.as_any(),
            |data, id| {
                Ok(data.children_of(id).map(|mut ids| {
                    ids.any(|c| {
                        data.effective_tag_of(c) == tags::PRIVATE && data.value_of(c) == "Y"
                    })
                }))
            },
            |children| {
                for child in children {
                    if child.tag_matches(py, tags::PRIVATE)? && child.value_str(py)? == "Y" {
                        return Ok(true);
                    }
                }
                Ok(false)
            },
        )
    }

    /// Returns an individual's names as a tuple: (`str` given_name, `str` surname)
    /// :rtype: tuple
    fn get_name(slf: &Bound<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let mut given_name = String::new();
        let mut surname = String::new();
        let mut found_given_name = false;
        let mut found_surname = false;

        if let Some((given, family)) = with_children(
            py,
            slf.as_any(),
            |data, id| Ok(native_get_name(data, id).map(Some)),
            |_| Ok(None),
        )? {
            return to_tuple(py, &given, &family);
        }

        for child in children_of(py, slf.as_any())? {
            if !child.tag_matches(py, tags::NAME)? {
                continue;
            }

            let value = child.value_str(py)?;
            if !value.is_empty() {
                let parts: Vec<&str> = value.split('/').collect();
                given_name = pystr::strip(parts[0]).to_owned();
                if parts.len() > 1 {
                    surname = pystr::strip(parts[1]).to_owned();
                }
                return to_tuple(py, &given_name, &surname);
            }

            for grandchild in child.children(py)? {
                let tag = grandchild.tag(py)?;
                if tag == tags::GIVEN_NAME {
                    given_name = grandchild.value_str(py)?;
                    found_given_name = true;
                }
                if tag == tags::SURNAME {
                    surname = grandchild.value_str(py)?;
                    found_surname = true;
                }
            }

            if found_given_name && found_surname {
                return to_tuple(py, &given_name, &surname);
            }
        }

        to_tuple(py, &given_name, &surname)
    }

    fn get_all_names(slf: &Bound<'_, Self>) -> PyResult<Py<PyList>> {
        let py = slf.py();
        let mut names: Vec<Py<PyAny>> = Vec::new();
        for child in children_of(py, slf.as_any())? {
            if child.tag_matches(py, tags::NAME)? {
                names.push(child.value(py)?);
            }
        }
        Ok(PyList::new(py, names)?.unbind())
    }

    /// Matches a string with the surname of an individual
    /// :type surname_to_match: str
    /// :rtype: bool
    fn surname_match(slf: &Bound<'_, Self>, surname_to_match: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let name = name_of(slf)?;
        let surname: String = name.bind(py).get_item(1)?.extract()?;
        regex_search(py, surname_to_match, &surname)
    }

    /// Matches a string with the given name of an individual
    /// :type given_name_to_match: str
    /// :rtype: bool
    fn given_name_match(
        slf: &Bound<'_, Self>,
        given_name_to_match: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let name = name_of(slf)?;
        let given_name: String = name.bind(py).get_item(0)?.extract()?;
        regex_search(py, given_name_to_match, &given_name)
    }

    /// Returns the gender of a person in string format
    /// :rtype: str
    fn get_gender(slf: &Bound<'_, Self>) -> PyResult<String> {
        last_child_value(slf.py(), slf.as_any(), tags::SEX)
    }

    /// Returns the birth data of a person formatted as a tuple: (`str` date, `str` place, `list` sources)
    /// :rtype: tuple
    fn get_birth_data(slf: &Bound<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let (date, place, sources) = event_data(py, slf.as_any(), tags::BIRTH)?;
        Ok(PyTuple::new(py, [
            date.into_pyobject(py)?.into_any().unbind(),
            place.into_pyobject(py)?.into_any().unbind(),
            PyList::new(py, sources)?.into_any().unbind(),
        ])?
        .into_any()
        .unbind())
    }

    /// Returns the birth year of a person in integer format
    /// :rtype: int
    fn get_birth_year(slf: &Bound<'_, Self>) -> PyResult<Py<PyAny>> {
        event_year(slf.py(), slf.as_any(), tags::BIRTH)
    }

    /// Returns the death data of a person formatted as a tuple: (`str` date, `str` place, `list` sources)
    /// :rtype: tuple
    fn get_death_data(slf: &Bound<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let (date, place, sources) = event_data(py, slf.as_any(), tags::DEATH)?;
        Ok(PyTuple::new(py, [
            date.into_pyobject(py)?.into_any().unbind(),
            place.into_pyobject(py)?.into_any().unbind(),
            PyList::new(py, sources)?.into_any().unbind(),
        ])?
        .into_any()
        .unbind())
    }

    /// Returns the death year of a person in integer format
    /// :rtype: int
    fn get_death_year(slf: &Bound<'_, Self>) -> PyResult<Py<PyAny>> {
        event_year(slf.py(), slf.as_any(), tags::DEATH)
    }

    /// Returns the burial data of a person formatted as a tuple: (`str` date, `str´ place, `list` sources)
    /// :rtype: tuple
    fn get_burial_data(slf: &Bound<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let (date, place, sources) = event_data(py, slf.as_any(), tags::BURIAL)?;
        Ok(PyTuple::new(py, [
            date.into_pyobject(py)?.into_any().unbind(),
            place.into_pyobject(py)?.into_any().unbind(),
            PyList::new(py, sources)?.into_any().unbind(),
        ])?
        .into_any()
        .unbind())
    }

    /// Returns a list of censuses of an individual formatted as tuples: (`str` date, `str´ place, `list` sources)
    /// :rtype: list of tuple
    fn get_census_data(slf: &Bound<'_, Self>) -> PyResult<Py<PyList>> {
        let py = slf.py();
        let mut census: Vec<Py<PyAny>> = Vec::new();

        for child in children_of(py, slf.as_any())? {
            if !child.tag_matches(py, tags::CENSUS)? {
                continue;
            }

            let mut date = String::new();
            let mut place = String::new();
            let mut sources: Vec<String> = Vec::new();

            for grandchild in child.children(py)? {
                let tag = grandchild.tag(py)?;
                if tag == tags::DATE {
                    date = grandchild.value_str(py)?;
                }
                if tag == tags::PLACE {
                    place = grandchild.value_str(py)?;
                }
                if tag == tags::SOURCE {
                    sources.push(grandchild.value_str(py)?);
                }
            }

            census.push(
                PyTuple::new(py, [
                    date.into_pyobject(py)?.into_any().unbind(),
                    place.into_pyobject(py)?.into_any().unbind(),
                    PyList::new(py, sources)?.into_any().unbind(),
                ])?
                .into_any()
                .unbind(),
            );
        }

        Ok(PyList::new(py, census)?.unbind())
    }

    /// Returns the date of when the person data was last changed formatted as a string
    /// :rtype: str
    fn get_last_change_date(slf: &Bound<'_, Self>) -> PyResult<String> {
        let py = slf.py();
        with_children(
            py,
            slf.as_any(),
            |data, id| {
                let Some(ids) = data.children_of(id) else {
                    return Ok(None);
                };
                let mut date = String::new();
                for child_id in ids {
                    if data.effective_tag_of(child_id) != tags::CHANGE {
                        continue;
                    }
                    let Some(grandchildren) = data.children_of(child_id) else {
                        return Ok(None);
                    };
                    for grandchild_id in grandchildren {
                        if data.effective_tag_of(grandchild_id) == tags::DATE {
                            date = data.value_of(grandchild_id).to_owned();
                        }
                    }
                }
                Ok(Some(date))
            },
            |children| {
                let mut date = String::new();
                for child in children {
                    if !child.tag_matches(py, tags::CHANGE)? {
                        continue;
                    }
                    for grandchild in child.children(py)? {
                        if grandchild.tag(py)? == tags::DATE {
                            date = grandchild.value_str(py)?;
                        }
                    }
                }
                Ok(date)
            },
        )
    }

    /// Returns the occupation of a person
    /// :rtype: str
    fn get_occupation(slf: &Bound<'_, Self>) -> PyResult<String> {
        last_child_value(slf.py(), slf.as_any(), tags::OCCUPATION)
    }

    /// Returns `True` if the given year matches the birth year of this person
    /// :type year: int
    /// :rtype: bool
    fn birth_year_match(slf: &Bound<'_, Self>, year: &Bound<'_, PyAny>) -> PyResult<bool> {
        event_year_of(slf, true)?.bind(slf.py()).eq(year)
    }

    /// Checks if the birth year of a person lies within the given range
    /// :type from_year: int
    /// :type to_year: int
    /// :rtype: bool
    fn birth_range_match(
        slf: &Bound<'_, Self>,
        from_year: &Bound<'_, PyAny>,
        to_year: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        let birth_year = event_year_of(slf, true)?;
        let birth_year = birth_year.bind(slf.py());
        Ok(from_year.le(birth_year)? && birth_year.le(to_year)?)
    }

    /// Returns `True` if the given year matches the death year of this person
    /// :type year: int
    /// :rtype: bool
    fn death_year_match(slf: &Bound<'_, Self>, year: &Bound<'_, PyAny>) -> PyResult<bool> {
        event_year_of(slf, false)?.bind(slf.py()).eq(year)
    }

    /// Checks if the death year of a person lies within the given range
    /// :type from_year: int
    /// :type to_year: int
    /// :rtype: bool
    fn death_range_match(
        slf: &Bound<'_, Self>,
        from_year: &Bound<'_, PyAny>,
        to_year: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        let death_year = event_year_of(slf, false)?;
        let death_year = death_year.bind(slf.py());
        Ok(from_year.le(death_year)? && death_year.le(to_year)?)
    }

    /// Checks if this individual matches all of the given criteria
    ///
    /// `criteria` is a colon-separated list, where each item in the
    /// list has the form [name]=[value]. The following criteria are supported:
    ///
    /// surname=[name]
    ///      Match a person with [name] in any part of the `surname`.
    /// given_name=[given_name]
    ///      Match a person with [given_name] in any part of the given `given_name`.
    /// birth=[year]
    ///      Match a person whose birth year is a four-digit [year].
    /// birth_range=[from_year-to_year]
    ///      Match a person whose birth year is in the range of years from
    ///      [from_year] to [to_year], including both [from_year] and [to_year].
    ///
    /// :type criteria: str
    /// :rtype: bool
    fn criteria_match(slf: &Bound<'_, Self>, criteria: &str) -> PyResult<bool> {
        let mut matched = true;

        for criterion in criteria.split(':') {
            let parts: Vec<&str> = criterion.split('=').collect();
            if parts.len() != 2 {
                return Err(PyValueError::new_err(format!(
                    "{} values to unpack (expected 2, got {})",
                    if parts.len() < 2 {
                        "not enough"
                    } else {
                        "too many"
                    },
                    parts.len()
                )));
            }
            let (key, value) = (parts[0], parts[1]);

            match key {
                "surname" => {
                    if !name_match_of(slf, Name::Surname, value)? {
                        matched = false;
                    }
                }
                "name" => {
                    if !name_match_of(slf, Name::Given, value)? {
                        matched = false;
                    }
                }
                "birth" => {
                    if !year_match(slf, Event::Birth, value)? {
                        matched = false;
                    }
                }
                "birth_range" => {
                    if !range_match(slf, Event::Birth, value)? {
                        matched = false;
                    }
                }
                "death" => {
                    if !year_match(slf, Event::Death, value)? {
                        matched = false;
                    }
                }
                "death_range" => {
                    if !range_match(slf, Event::Death, value)? {
                        matched = false;
                    }
                }
                _ => {}
            }
        }

        Ok(matched)
    }
}

#[derive(Clone, Copy)]
enum Event {
    Birth,
    Death,
}

#[derive(Clone, Copy)]
enum Name {
    Surname,
    Given,
}

fn name_match_of(slf: &Bound<'_, IndividualElement>, which: Name, value: &str) -> PyResult<bool> {
    let py = slf.py();
    if is_plain(slf) {
        let name = IndividualElement::get_name(slf)?;
        let index = match which {
            Name::Surname => 1,
            Name::Given => 0,
        };
        let text: String = name.bind(py).get_item(index)?.extract()?;
        let pattern = value.into_pyobject(py)?;
        return Ok(!regex_search(py, pattern.as_any(), &text)?.is_none(py));
    }

    let method = match which {
        Name::Surname => "surname_match",
        Name::Given => "given_name_match",
    };
    slf.as_any().call_method1(method, (value,))?.is_truthy()
}

fn year_match(slf: &Bound<'_, IndividualElement>, event: Event, value: &str) -> PyResult<bool> {
    let py = slf.py();
    let Some(year) = pystr::python_int(py, value)? else {
        return Ok(false);
    };
    let year = year.bind(py);

    if is_plain(slf) {
        let actual = event_year_of(slf, matches!(event, Event::Birth))?;
        return actual.bind(py).eq(year);
    }

    let method = match event {
        Event::Birth => "birth_year_match",
        Event::Death => "death_year_match",
    };
    match slf.as_any().call_method1(method, (year,)) {
        Ok(result) => result.is_truthy(),
        Err(error) if error.is_instance_of::<PyValueError>(py) => Ok(false),
        Err(error) => Err(error),
    }
}

fn range_match(slf: &Bound<'_, IndividualElement>, event: Event, value: &str) -> PyResult<bool> {
    let py = slf.py();
    let parts: Vec<&str> = value.split('-').collect();
    if parts.len() != 2 {
        return Ok(false);
    }

    let (Some(from_year), Some(to_year)) = (
        pystr::python_int(py, parts[0])?,
        pystr::python_int(py, parts[1])?,
    ) else {
        return Ok(false);
    };
    let (from_year, to_year) = (from_year.bind(py), to_year.bind(py));

    if is_plain(slf) {
        let actual = event_year_of(slf, matches!(event, Event::Birth))?;
        let actual = actual.bind(py);
        return Ok(from_year.le(actual)? && actual.le(to_year)?);
    }

    let method = match event {
        Event::Birth => "birth_range_match",
        Event::Death => "death_range_match",
    };
    match slf.as_any().call_method1(method, (from_year, to_year)) {
        Ok(result) => result.is_truthy(),
        Err(error) if error.is_instance_of::<PyValueError>(py) => Ok(false),
        Err(error) => Err(error),
    }
}

fn last_child_value(py: Python<'_>, object: &Bound<'_, PyAny>, tag: &str) -> PyResult<String> {
    with_children(
        py,
        object,
        |data, id| {
            let Some(ids) = data.children_of(id) else {
                return Ok(None);
            };
            let mut found = String::new();
            for child_id in ids {
                if data.effective_tag_of(child_id) == tag {
                    found = data.value_of(child_id).to_owned();
                }
            }
            Ok(Some(found))
        },
        |children| {
            let mut found = String::new();
            for child in children {
                if child.tag_matches(py, tag)? {
                    found = child.value_str(py)?;
                }
            }
            Ok(found)
        },
    )
}

fn native_get_name(data: &ArenaData, id: u32) -> Option<(String, String)> {
    let mut given_name = String::new();
    let mut surname = String::new();
    let mut found_given_name = false;
    let mut found_surname = false;

    for child_id in data.children_of(id)? {
        if data.effective_tag_of(child_id) != tags::NAME {
            continue;
        }

        let value = data.value_of(child_id);
        if !value.is_empty() {
            let parts: Vec<&str> = value.split('/').collect();
            given_name = pystr::strip(parts[0]).to_owned();
            if parts.len() > 1 {
                surname = pystr::strip(parts[1]).to_owned();
            }
            return Some((given_name, surname));
        }

        for grandchild_id in data.children_of(child_id)? {
            match data.effective_tag_of(grandchild_id) {
                tags::GIVEN_NAME => {
                    given_name = data.value_of(grandchild_id).to_owned();
                    found_given_name = true;
                }
                tags::SURNAME => {
                    surname = data.value_of(grandchild_id).to_owned();
                    found_surname = true;
                }
                _ => {}
            }
        }

        if found_given_name && found_surname {
            return Some((given_name, surname));
        }
    }

    Some((given_name, surname))
}

pub fn require_individual(object: &Bound<'_, PyAny>) -> PyResult<()> {
    if object.is_instance_of::<IndividualElement>() {
        return Ok(());
    }
    Err(NotAnActualIndividualError::new_err(format!(
        "Operation only valid for elements with {} tag",
        tags::INDIVIDUAL
    )))
}
