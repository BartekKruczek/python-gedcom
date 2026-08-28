//! `Element` and its built-in subclasses.

use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::PyList;
use pyo3::types::PyString;
use pyo3::PyTraverseError;
use pyo3::PyVisit;

use crate::arena::{Arena, ArenaData, Kind};
use crate::bounded;
use crate::individual::IndividualElement;
use crate::tags;

pub fn make_handle(py: Python<'_>, arena: &Py<Arena>, id: u32) -> PyResult<Py<PyAny>> {
    if let Some(existing) = arena.get().cached_handle(py, id) {
        return Ok(existing);
    }
    Ok(make_handles(py, arena, &[id])?.remove(0))
}

pub fn make_handles(py: Python<'_>, arena: &Py<Arena>, ids: &[u32]) -> PyResult<Vec<Py<PyAny>>> {
    let mut handles: Vec<Option<Py<PyAny>>> = Vec::with_capacity(ids.len());
    let mut kinds: Vec<Kind> = Vec::with_capacity(ids.len());

    {
        let borrowed = arena.get();
        let data = borrowed.read();
        for id in ids {
            let cached = data
                .handles
                .get(*id as usize)
                .and_then(|slot| slot.as_ref())
                .map(|handle| handle.clone_ref(py));
            kinds.push(data.node(*id).kind);
            handles.push(cached);
        }
    }

    let mut created: Vec<(u32, Py<PyAny>)> = Vec::new();
    for (index, id) in ids.iter().enumerate() {
        if handles[index].is_some() {
            continue;
        }
        let object = build_handle(py, arena, *id, kinds[index])?;
        created.push((*id, object.clone_ref(py)));
        handles[index] = Some(object);
    }

    if !created.is_empty() {
        let borrowed = arena.get();
        let mut data = borrowed.write();
        for (id, object) in created {
            if let Some(slot) = data.handles.get_mut(id as usize) {
                if slot.is_none() {
                    *slot = Some(object);
                }
            }
        }
    }

    Ok(handles.into_iter().map(|handle| handle.unwrap()).collect())
}

fn build_handle(py: Python<'_>, arena: &Py<Arena>, id: u32, kind: Kind) -> PyResult<Py<PyAny>> {
    let handle = Element {
        arena: arena.clone_ref(py),
        id,
    };
    Ok(match kind {
        Kind::Base => Py::new(py, handle)?.into_any(),
        Kind::Individual => {
            Py::new(py, PyClassInitializer::from(handle).add_subclass(IndividualElement))?.into_any()
        }
        Kind::Family => {
            Py::new(py, PyClassInitializer::from(handle).add_subclass(FamilyElement))?.into_any()
        }
        Kind::File => {
            Py::new(py, PyClassInitializer::from(handle).add_subclass(FileElement))?.into_any()
        }
        Kind::Object => {
            Py::new(py, PyClassInitializer::from(handle).add_subclass(ObjectElement))?.into_any()
        }
        Kind::Root => {
            Py::new(py, PyClassInitializer::from(handle).add_subclass(RootElement))?.into_any()
        }
    })
}

pub fn ensure_registered(py: Python<'_>, object: &Bound<'_, PyAny>) -> PyResult<()> {
    let Some((arena, id)) = element_parts(py, object) else {
        return Ok(());
    };
    let borrowed = arena.get();
    if borrowed.cached_handle(py, id).is_some() {
        return Ok(());
    }
    borrowed.remember_handle(id, object)
}

pub fn element_parts(py: Python<'_>, object: &Bound<'_, PyAny>) -> Option<(Py<Arena>, u32)> {
    let element = object.cast::<Element>().ok()?;
    let borrowed = element.borrow();
    Some((borrowed.arena.clone_ref(py), borrowed.id))
}

pub fn is_builtin_element(object: &Bound<'_, PyAny>) -> bool {
    object.is_exact_instance_of::<Element>()
        || object.is_exact_instance_of::<IndividualElement>()
        || object.is_exact_instance_of::<FamilyElement>()
        || object.is_exact_instance_of::<FileElement>()
        || object.is_exact_instance_of::<ObjectElement>()
        || object.is_exact_instance_of::<RootElement>()
}

pub enum Child {
    Local { arena: Py<Arena>, id: u32 },
    Object(Py<PyAny>),
}

impl Child {
    pub fn tag_matches(&self, py: Python<'_>, wanted: &str) -> PyResult<bool> {
        match self {
            Child::Local { arena, id } => {
                let data = arena.get().read();
                Ok(data.effective_tag_of(*id) == wanted)
            }
            Child::Object(object) => {
                let tag = object.bind(py).call_method0("get_tag")?;
                Ok(tag.eq(PyString::new(py, wanted))?)
            }
        }
    }

    pub fn tag(&self, py: Python<'_>) -> PyResult<String> {
        match self {
            Child::Local { arena, id } => {
                let data = arena.get().read();
                Ok(data.effective_tag_of(*id).to_owned())
            }
            Child::Object(object) => object.bind(py).call_method0("get_tag")?.extract(),
        }
    }

    pub fn value(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match self {
            Child::Local { arena, id } => {
                let data = arena.get().read();
                Ok(PyString::new(py, data.value_of(*id)).into_any().unbind())
            }
            Child::Object(object) => Ok(object.bind(py).call_method0("get_value")?.unbind()),
        }
    }

    pub fn value_str(&self, py: Python<'_>) -> PyResult<String> {
        match self {
            Child::Local { arena, id } => {
                let data = arena.get().read();
                Ok(data.value_of(*id).to_owned())
            }
            Child::Object(object) => object.bind(py).call_method0("get_value")?.extract(),
        }
    }

    pub fn children(&self, py: Python<'_>) -> PyResult<Vec<Child>> {
        match self {
            Child::Local { arena, id } => child_view(py, arena, *id),
            Child::Object(object) => {
                let list = object.bind(py).call_method0("get_child_elements")?;
                let mut children = Vec::new();
                for item in list.try_iter()? {
                    children.push(Child::Object(item?.unbind()));
                }
                Ok(children)
            }
        }
    }

    pub fn handle(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match self {
            Child::Local { arena, id } => make_handle(py, arena, *id),
            Child::Object(object) => Ok(object.clone_ref(py)),
        }
    }
}

pub fn collect_descendants(data: &ArenaData, root: u32, out: &mut Vec<u32>) -> bool {
    let Some(top) = data.children_of(root) else {
        return false;
    };
    let mut stack: Vec<u32> = top.collect();
    stack.reverse();

    while let Some(id) = stack.pop() {
        out.push(id);
        let Some(children) = data.children_of(id) else {
            return false;
        };
        let mark = stack.len();
        stack.extend(children);
        stack[mark..].reverse();
    }

    true
}

pub fn with_children<T>(
    py: Python<'_>,
    object: &Bound<'_, PyAny>,
    native: impl FnOnce(&ArenaData, u32) -> PyResult<Option<T>>,
    fallback: impl FnOnce(Vec<Child>) -> PyResult<T>,
) -> PyResult<T> {
    if is_builtin_element(object) {
        if let Ok(element) = object.cast::<Element>() {
            let (arena, id) = {
                let borrowed = element.borrow();
                (borrowed.arena.clone_ref(py), borrowed.id)
            };
            let arena_ref = arena.get();
            let data = arena_ref.read();
            if let Some(result) = native(&data, id)? {
                return Ok(result);
            }
        }
    }
    fallback(children_of(py, object)?)
}

pub fn children_of(py: Python<'_>, object: &Bound<'_, PyAny>) -> PyResult<Vec<Child>> {
    if is_builtin_element(object) {
        if let Some((arena, id)) = element_parts(py, object) {
            return child_view(py, &arena, id);
        }
    }
    let list = object.call_method0("get_child_elements")?;
    let mut children = Vec::new();
    for item in list.try_iter()? {
        children.push(Child::Object(item?.unbind()));
    }
    Ok(children)
}

pub fn child_view(py: Python<'_>, arena: &Py<Arena>, id: u32) -> PyResult<Vec<Child>> {
    let materialized = {
        let data = arena.get().read();
        match data.children_of(id) {
            Some(children) => {
                return Ok(children
                    .map(|child_id| Child::Local {
                        arena: arena.clone_ref(py),
                        id: child_id,
                    })
                    .collect());
            }
            None => data.materialized_list(id).map(|list| list.clone_ref(py)),
        }
    };

    let Some(list) = materialized else {
        return Ok(Vec::new());
    };
    let bound = list.bind(py);
    let mut children = Vec::with_capacity(bound.len());
    for item in bound.iter() {
        children.push(Child::Object(item.unbind()));
    }
    Ok(children)
}

#[pyclass(subclass, weakref, module = "gedcom.element.element")]
/// GEDCOM element
///
/// Each line in a GEDCOM file is an element with the format
///
/// `level [pointer] tag [value]`
///
/// where `level` and `tag` are required, and `pointer` and `value` are
/// optional.  Elements are arranged hierarchically according to their
/// level, and elements with a level of zero are at the top level.
/// Elements with a level greater than zero are children of their
/// parent.
///
/// A pointer has the format `@pname@`, where `pname` is any sequence of
/// characters and numbers. The pointer identifies the object being
/// pointed to, so that any pointer included as the value of any
/// element points back to the original object.  For example, an
/// element may have a `FAMS` tag whose value is `@F1@`, meaning that this
/// element points to the family record in which the associated person
/// is a spouse. Likewise, an element with a tag of `FAMC` has a value
/// that points to a family record in which the associated person is a
/// child.
///
/// See a GEDCOM file for examples of tags and their values.
///
/// Tags available to an element are seen here: `gedcom.tags`
pub struct Element {
    pub arena: Py<Arena>,
    pub id: u32,
}

impl Element {
    pub fn create(
        py: Python<'_>,
        level: i64,
        pointer: Option<String>,
        tag: String,
        value: String,
        crlf: String,
        kind: Kind,
    ) -> PyResult<Element> {
        let arena = Py::new(py, Arena::empty())?;
        let id = {
            let mut data = arena.get().write();
            data.push_owned(level, pointer.as_deref(), &tag, &value, &crlf, kind)?
        };
        Ok(Element { arena, id })
    }

    fn crlf(&self, _py: Python<'_>) -> String {
        self.arena.get().read().crlf_of(self.id).to_owned()
    }

    fn level(&self, _py: Python<'_>) -> i64 {
        self.arena.get().read().node(self.id).level
    }
}

fn render_line(
    level: i64,
    pointer: Option<&str>,
    has_pointer_field: bool,
    tag: &str,
    value: &str,
    crlf: &str,
    out: &mut String,
) -> PyResult<()> {
    if level < 0 {
        if has_pointer_field && pointer.is_none() {
            return Err(none_pointer_error());
        }
        return Ok(());
    }

    push_level(level, out);

    match pointer {
        Some("") => {}
        Some(text) => {
            out.push(' ');
            out.push_str(text);
        }
        None => return Err(none_pointer_error()),
    }

    out.push(' ');
    out.push_str(tag);

    if !value.is_empty() {
        out.push(' ');
        out.push_str(value);
    }

    out.push_str(crlf);
    Ok(())
}

fn push_level(level: i64, out: &mut String) {
    if (0..10).contains(&level) {
        out.push((b'0' + level as u8) as char);
        return;
    }

    let mut digits = [0u8; 20];
    let mut index = digits.len();
    let negative = level < 0;
    let mut remaining = level.unsigned_abs();

    while remaining > 0 {
        index -= 1;
        digits[index] = b'0' + (remaining % 10) as u8;
        remaining /= 10;
    }
    if negative {
        index -= 1;
        digits[index] = b'-';
    }

    out.push_str(std::str::from_utf8(&digits[index..]).expect("ascii digits"));
}

fn none_pointer_error() -> PyErr {
    PyTypeError::new_err("can only concatenate str (not \"NoneType\") to str")
}

fn render_subtree(
    py: Python<'_>,
    arena: &Py<Arena>,
    root_id: u32,
    recursive: bool,
    out: &mut String,
) -> PyResult<()> {
    let mut stack: Vec<Pending> = vec![Pending::Local {
        arena: arena.clone_ref(py),
        id: root_id,
    }];

    while let Some(item) = stack.pop() {
        match item {
            Pending::Foreign(object) => {
                let rendered: String = object
                    .bind(py)
                    .call_method1("to_gedcom_string", (true,))?
                    .extract()?;
                out.push_str(&rendered);
            }
            Pending::Local { arena, id } => {
                let materialized = {
                    let data = arena.get().read();
                    render_line(
                        data.node(id).level,
                        data.pointer_of(id),
                        true,
                        data.effective_tag_of(id),
                        data.value_of(id),
                        data.crlf_of(id),
                        out,
                    )?;

                    if !recursive {
                        return Ok(());
                    }

                    match data.children_of(id) {
                        Some(children) => {
                            let mark = stack.len();
                            for child_id in children {
                                stack.push(Pending::Local {
                                    arena: arena.clone_ref(py),
                                    id: child_id,
                                });
                            }
                            stack[mark..].reverse();
                            None
                        }
                        None => data.materialized_list(id).map(|list| list.clone_ref(py)),
                    }
                };

                if let Some(list) = materialized {
                    let bound = list.bind(py);
                    let mut pending = Vec::with_capacity(bound.len());
                    for child in bound.iter() {
                        pending.push(classify(py, &child));
                    }
                    for item in pending.into_iter().rev() {
                        stack.push(item);
                    }
                }
            }
        }
    }

    Ok(())
}

fn render_native(
    data: &ArenaData,
    root: u32,
    recursive: bool,
    out: &mut String,
) -> PyResult<()> {
    let mut stack = vec![root];

    while let Some(id) = stack.pop() {
        render_line(
            data.node(id).level,
            data.pointer_of(id),
            true,
            data.effective_tag_of(id),
            data.value_of(id),
            data.crlf_of(id),
            out,
        )?;

        if !recursive {
            return Ok(());
        }

        let mark = stack.len();
        stack.extend(data.children_of(id).expect("subtree checked as native"));
        stack[mark..].reverse();
    }

    Ok(())
}

enum Pending {
    Local { arena: Py<Arena>, id: u32 },
    Foreign(Py<PyAny>),
}

fn classify(py: Python<'_>, object: &Bound<'_, PyAny>) -> Pending {
    if is_builtin_element(object) {
        if let Some((arena, id)) = element_parts(py, object) {
            return Pending::Local { arena, id };
        }
    }
    Pending::Foreign(object.clone().unbind())
}

#[pymethods]
impl Element {
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
    ) -> PyResult<Element> {
        let kind = Kind::Base;
        let element = Element::create(py, level, pointer, tag, value.clone(), crlf.to_owned(), kind)?;
        if multi_line {
            element.apply_multi_line_value(py, None, &value)?;
        }
        Ok(element)
    }

    /// Returns the level of this element from within the GEDCOM file
    /// :rtype: int
    fn get_level(&self, py: Python<'_>) -> i64 {
        self.level(py)
    }

    /// Returns the pointer of this element from within the GEDCOM file
    /// :rtype: str or None
    pub fn get_pointer(&self, _py: Python<'_>) -> Option<String> {
        self.arena
            .get()
            .read()
            .pointer_of(self.id)
            .map(|text| text.to_owned())
    }

    /// Returns the tag of this element from within the GEDCOM file
    /// :rtype: str
    fn get_tag(&self, _py: Python<'_>) -> String {
        self.arena.get().read().effective_tag_of(self.id).to_owned()
    }

    /// Return the value of this element from within the GEDCOM file
    /// :rtype: str
    fn get_value(&self, _py: Python<'_>) -> String {
        self.arena.get().read().value_of(self.id).to_owned()
    }

    /// Sets the value of this element
    /// :type value: str
    fn set_value(&self, _py: Python<'_>, value: String) -> PyResult<()> {
        self.arena.get().write().set_value_of(self.id, &value)
    }

    /// Returns the direct child elements of this element
    /// :rtype: list of Element
    pub fn get_child_elements(&self, py: Python<'_>) -> PyResult<Py<PyList>> {
        let ids: Vec<u32> = {
            let data = self.arena.get().read();
            match data.children_of(self.id) {
                Some(children) => children.collect(),
                None => {
                    return Ok(data
                        .materialized_list(self.id)
                        .expect("materialised node without a list")
                        .clone_ref(py))
                }
            }
        };

        let handles = make_handles(py, &self.arena, &ids)?;
        let list = PyList::new(py, handles)?;

        let mut data = self.arena.get().write();

        if let Some(existing) = data.materialized_list(self.id) {
            return Ok(existing.clone_ref(py));
        }
        let stored = list.clone().unbind();
        data.set_materialized_list(self.id, stored.clone_ref(py));
        Ok(stored)
    }

    /// Creates and returns a new child element of this element
    ///
    /// :type tag: str
    /// :type pointer: str
    /// :type value: str
    /// :rtype: Element
    #[pyo3(signature = (tag, pointer = "", value = ""))]
    fn new_child_element(
        slf: &Bound<'_, Self>,
        tag: String,
        pointer: &str,
        value: &str,
    ) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let borrowed = slf.borrow();
        let crlf = borrowed.crlf(py);

        let level: i64 = if is_builtin_element(slf.as_any()) {
            borrowed.level(py)
        } else {
            slf.as_any().call_method0("get_level")?.extract()?
        };

        let arena = borrowed.arena.clone_ref(py);
        let parent_id = borrowed.id;
        drop(borrowed);

        let kind = Kind::for_tag(&tag);
        let child_id = {
            let arena_ref = arena.get();
            let mut data = arena_ref.write();
            data.push_owned(level + 1, Some(pointer), &tag, value, &crlf, kind)?
        };

        let child = make_handle(py, &arena, child_id)?;
        {
            let bound = child.bind(py);
            let child_ref = bound.cast::<Element>()?.borrow();
            child_ref.apply_multi_line_value(py, None, value)?;
        }

        attach_child(py, &arena, parent_id, slf.as_any(), child.bind(py))?;
        Ok(child)
    }

    /// Adds a child element to this element
    ///
    /// :type element: Element
    fn add_child_element(
        slf: &Bound<'_, Self>,
        element: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let borrowed = slf.borrow();
        let arena = borrowed.arena.clone_ref(py);
        let parent_id = borrowed.id;
        drop(borrowed);

        attach_child(py, &arena, parent_id, slf.as_any(), element)?;
        Ok(element.clone().unbind())
    }

    /// Returns the parent element of this element
    /// :rtype: Element or None
    fn get_parent_element(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let parent = {
            let data = self.arena.get().read();
            if let Some(object) = data.foreign_parent_of(self.id) {
                return Ok(object.clone_ref(py));
            }
            match data.parent_of(self.id) {
                Some(id) => id,
                None => return Ok(py.None()),
            }
        };
        make_handle(py, &self.arena, parent)
    }

    /// Adds a parent element to this element
    ///
    /// There's usually no need to call this method manually,
    /// `add_child_element()` calls it automatically.
    ///
    /// :type element: Element
    fn set_parent_element(&self, py: Python<'_>, element: &Bound<'_, PyAny>) -> PyResult<()> {
        ensure_registered(py, element)?;
        let mut data = self.arena.get().write();
        if element.is_none() {
            data.clear_parent(self.id);
            return Ok(());
        }
        if let Some((other, id)) = element_parts(py, element) {
            if other.as_ptr() == self.arena.as_ptr() {
                data.set_local_parent(self.id, id);
                return Ok(());
            }
        }
        data.set_foreign_parent(self.id, element.clone().unbind());
        Ok(())
    }

    /// Returns the value of this element including concatenations or continuations
    /// :rtype: str
    fn get_multi_line_value(slf: &Bound<'_, Self>) -> PyResult<String> {
        let py = slf.py();
        let borrowed = slf.borrow();
        let arena = borrowed.arena.clone_ref(py);
        let id = borrowed.id;

        let mut result = borrowed.get_value(py);
        let mut last_crlf = borrowed.crlf(py);
        drop(borrowed);

        for child in child_view(py, &arena, id)? {
            let tag = child.tag(py)?;
            if tag == tags::CONCATENATION {
                result.push_str(&child.value_str(py)?);
                last_crlf = child_crlf(py, &child)?;
            } else if tag == tags::CONTINUED {
                result.push_str(&last_crlf);
                result.push_str(&child.value_str(py)?);
                last_crlf = child_crlf(py, &child)?;
            }
        }

        Ok(result)
    }

    /// Sets the value of this element, adding concatenation and continuation lines when necessary
    /// :type value: str
    fn set_multi_line_value(slf: &Bound<'_, Self>, value: String) -> PyResult<()> {
        let borrowed = slf.borrow();
        borrowed.apply_multi_line_value(slf.py(), Some(slf.as_any()), &value)
    }

    /// Formats this element and optionally all of its sub-elements into a GEDCOM string
    /// :type recursive: bool
    /// :rtype: str
    #[pyo3(signature = (recursive = false))]
    fn to_gedcom_string(slf: &Bound<'_, Self>, recursive: bool) -> PyResult<String> {
        let py = slf.py();

        if !is_builtin_element(slf.as_any()) {
            return generic_to_gedcom_string(py, slf.as_any(), recursive);
        }

        let (arena, id) = {
            let borrowed = slf.borrow();
            (borrowed.arena.clone_ref(py), borrowed.id)
        };

        if !recursive {
            let data = arena.get().read();
            let mut out = String::with_capacity(64);
            render_native(&data, id, false, &mut out)?;
            return Ok(out);
        }

        let data = arena.get().read();
        if let Some(capacity) = data.subtree_metrics(id) {
            let mut out = String::with_capacity(capacity);
            render_native(&data, id, true, &mut out)?;
            return Ok(out);
        }
        drop(data);

        let mut out = String::new();
        render_subtree(py, &arena, id, true, &mut out)?;
        Ok(out)
    }

    /// :rtype: str
    fn __str__(slf: &Bound<'_, Self>) -> PyResult<String> {
        if is_builtin_element(slf.as_any()) {
            return Element::to_gedcom_string(slf, false);
        }
        slf.as_any().call_method0("to_gedcom_string")?.extract()
    }

    /// Two handles onto the same node are the same element.

    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> bool {
        match other.cast::<Element>() {
            Ok(other) => {
                let other = other.borrow();
                self.id == other.id && self.arena.as_ptr() == other.arena.as_ptr()
            }
            Err(_) => {
                let _ = py;
                false
            }
        }
    }

    fn __hash__(&self) -> u64 {
        (self.arena.as_ptr() as u64) ^ (self.id as u64).rotate_left(32)
    }

    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.arena)
    }
}

impl Element {
    pub fn apply_multi_line_value(
        &self,
        py: Python<'_>,
        owner: Option<&Bound<'_, PyAny>>,
        value: &str,
    ) -> PyResult<()> {
        self.set_value(py, String::new())?;
        self.drop_continuation_children(py)?;

        let characters: Vec<char> = value.chars().collect();
        let mut lines = bounded::splitlines(&characters);
        if lines.is_empty() {
            return Ok(());
        }

        let first = lines.remove(0);
        let consumed = self.set_bounded_value(py, owner, &first)?;
        self.add_concatenation(py, owner, &first[consumed..])?;

        for line in lines {
            let consumed = self.add_bounded_child(py, owner, tags::CONTINUED, &line)?;
            self.add_concatenation(py, owner, &line[consumed..])?;
        }

        Ok(())
    }

    fn drop_continuation_children(&self, py: Python<'_>) -> PyResult<()> {
        let is_materialized = self.arena.get().read().node(self.id).is_materialized();

        if !is_materialized {
            let mut data = self.arena.get().write();
            data.retain_children(self.id, |data, child_id| {
                let tag = data.effective_tag_of(child_id);
                tag != tags::CONCATENATION && tag != tags::CONTINUED
            });
            return Ok(());
        }

        let list = self.get_child_elements(py)?;
        let bound = list.bind(py);
        let mut kept: Vec<Py<PyAny>> = Vec::with_capacity(bound.len());
        for item in bound.iter() {
            let tag = item.call_method0("get_tag")?;
            let tag: String = tag.extract()?;
            if tag != tags::CONCATENATION && tag != tags::CONTINUED {
                kept.push(item.unbind());
            }
        }
        bound.call_method1("__setitem__", (pyo3::types::PySlice::full(py), kept))?;
        Ok(())
    }

    fn available_characters(
        &self,
        _py: Python<'_>,
        owner: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<usize> {
        let rendered: String = match owner {
            Some(object) if !is_builtin_element(object) => {
                object.call_method0("to_gedcom_string")?.extract()?
            }
            _ => {
                let data = self.arena.get().read();
                let mut out = String::new();
                render_line(
                    data.node(self.id).level,
                    data.pointer_of(self.id),
                    true,
                    data.effective_tag_of(self.id),
                    data.value_of(self.id),
                    data.crlf_of(self.id),
                    &mut out,
                )?;
                out
            }
        };
        let limit = self.arena.get().read().line_limit();
        Ok(bounded::available_characters(rendered.chars().count(), limit))
    }

    fn set_bounded_value(
        &self,
        py: Python<'_>,
        owner: Option<&Bound<'_, PyAny>>,
        line: &[char],
    ) -> PyResult<usize> {
        let available = self.available_characters(py, owner)?;
        let length = bounded::line_length(line, available);
        let text: String = line[..length].iter().collect();
        self.set_value(py, text)?;
        Ok(length)
    }

    fn add_bounded_child(
        &self,
        py: Python<'_>,
        owner: Option<&Bound<'_, PyAny>>,
        tag: &str,
        line: &[char],
    ) -> PyResult<usize> {
        let child = match owner {
            Some(object) if !is_builtin_element(object) => {
                object.call_method1("new_child_element", (tag,))?
            }
            _ => {
                let handle = self.create_child(py, tag)?;
                handle.into_bound(py)
            }
        };

        let child_ref = child.cast::<Element>()?.borrow();
        child_ref.set_bounded_value(py, Some(&child), line)
    }

    fn add_concatenation(
        &self,
        py: Python<'_>,
        owner: Option<&Bound<'_, PyAny>>,
        text: &[char],
    ) -> PyResult<()> {
        let size = text.len();
        let mut index = 0usize;
        let mut iterations = 0u32;

        while index < size {
            index += self.add_bounded_child(py, owner, tags::CONCATENATION, &text[index..])?;
            iterations += 1;
            if iterations % 1024 == 0 {
                py.check_signals()?;
            }
        }

        Ok(())
    }

    fn create_child(&self, py: Python<'_>, tag: &str) -> PyResult<Py<PyAny>> {
        let (level, crlf) = {
            let data = self.arena.get().read();
            (data.node(self.id).level, data.crlf_of(self.id).to_owned())
        };

        let child_id = {
            let mut data = self.arena.get().write();
            let id = data.push_owned(level + 1, Some(""), tag, "", &crlf, Kind::for_tag(tag))?;
            data.set_local_parent(id, self.id);
            id
        };

        append_child_id(py, &self.arena, self.id, child_id)?;
        make_handle(py, &self.arena, child_id)
    }
}

fn child_crlf(py: Python<'_>, child: &Child) -> PyResult<String> {
    match child {
        Child::Local { arena, id } => {
            let data = arena.get().read();
            Ok(data.crlf_of(*id).to_owned())
        }
        Child::Object(object) => {
            let bound = object.bind(py);
            match bound.cast::<Element>() {
                Ok(element) => {
                    let element = element.borrow();
                    Ok(element.crlf(py))
                }
                Err(_) => Err(pyo3::exceptions::PyAttributeError::new_err(format!(
                    "'{}' object has no attribute '_Element__crlf'",
                    bound.get_type().name()?
                ))),
            }
        }
    }
}

fn attach_child(
    py: Python<'_>,
    arena: &Py<Arena>,
    parent_id: u32,
    parent_object: &Bound<'_, PyAny>,
    element: &Bound<'_, PyAny>,
) -> PyResult<()> {
    ensure_registered(py, parent_object)?;
    ensure_registered(py, element)?;

    let same_arena = element_parts(py, element)
        .map(|(other, id)| (other.as_ptr() == arena.as_ptr(), id))
        .filter(|(same, _)| *same)
        .map(|(_, id)| id);

    let already_materialized = arena.get().read().node(parent_id).is_materialized();

    match same_arena {
        Some(child_id) if !already_materialized => {
            append_child_id(py, arena, parent_id, child_id)?;
        }
        _ => {
            let list = {
                let parent = parent_object.cast::<Element>()?.borrow();
                parent.get_child_elements(py)?
            };
            list.bind(py).append(element)?;
        }
    }

    element.call_method1("set_parent_element", (parent_object,))?;
    Ok(())
}

fn append_child_id(
    py: Python<'_>,
    arena: &Py<Arena>,
    parent_id: u32,
    child_id: u32,
) -> PyResult<()> {
    {
        let mut data = arena.get().write();
        if !data.node(parent_id).is_materialized() {
            data.append_child(parent_id, child_id);
            return Ok(());
        }
    }

    let handle = make_handle(py, arena, child_id)?;
    let list = arena
        .get()
        .read()
        .materialized_list(parent_id)
        .map(|list| list.clone_ref(py));
    if let Some(list) = list {
        list.bind(py).append(handle)?;
    }
    Ok(())
}

fn generic_to_gedcom_string(
    py: Python<'_>,
    object: &Bound<'_, PyAny>,
    recursive: bool,
) -> PyResult<String> {
    let level: i64 = object.call_method0("get_level")?.extract()?;
    let pointer = object.call_method0("get_pointer")?;
    let tag: String = object.call_method0("get_tag")?.extract()?;
    let value: String = object.call_method0("get_value")?.extract()?;

    let crlf = match object.cast::<Element>() {
        Ok(element) => element.borrow().crlf(py),
        Err(_) => "\n".to_owned(),
    };

    let pointer_text: Option<String> = if pointer.is_none() {
        None
    } else {
        Some(pointer.extract().map_err(|_| none_pointer_error())?)
    };

    let mut out = String::new();
    render_line(
        level,
        pointer_text.as_deref(),
        true,
        &tag,
        &value,
        &crlf,
        &mut out,
    )?;

    if recursive {
        let children = object.call_method0("get_child_elements")?;
        for child in children.try_iter()? {
            let child = child?;
            let rendered: String = child.call_method1("to_gedcom_string", (true,))?.extract()?;
            out.push_str(&rendered);
        }
    }

    Ok(out)
}

macro_rules! element_subclass {
    ($name:ident, $kind:expr, $module:literal $(, tag = $tag:expr)?) => {
        #[pyclass(extends = Element, subclass, module = $module)]
        pub struct $name;

        #[pymethods]
        impl $name {
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
                let element =
                    Element::create(py, level, pointer, tag, value.clone(), crlf.to_owned(), $kind)?;
                if multi_line {
                    element.apply_multi_line_value(py, None, &value)?;
                }
                Ok(PyClassInitializer::from(element).add_subclass($name))
            }

            $(
                fn get_tag(&self) -> &'static str {
                    $tag
                }
            )?
        }
    };
}

element_subclass!(
    FamilyElement,
    Kind::Family,
    "gedcom.element.family",
    tag = tags::FAMILY
);
element_subclass!(
    FileElement,
    Kind::File,
    "gedcom.element.file",
    tag = tags::FILE
);
/// GEDCOM element consisting of tag `OBJE`.
#[pyclass(extends = Element, subclass, module = "gedcom.element.object")]
pub struct ObjectElement;

#[pymethods]
impl ObjectElement {
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
        let element = Element::create(py, level, pointer, tag, value.clone(), crlf.to_owned(), Kind::Object)?;
        if multi_line {
            element.apply_multi_line_value(py, None, &value)?;
        }
        Ok(PyClassInitializer::from(element).add_subclass(ObjectElement))
    }

    /// Checks if this element is an actual object
    /// :rtype: bool
    fn is_object(slf: &Bound<'_, Self>) -> PyResult<bool> {
        if is_builtin_element(slf.as_any()) {
            let element = slf.as_super();
            return Ok(element.borrow().get_tag(slf.py()) == tags::OBJECT);
        }
        let tag: String = slf.as_any().call_method0("get_tag")?.extract()?;
        Ok(tag == tags::OBJECT)
    }
}

/// Virtual GEDCOM root element containing all logical records as children
#[pyclass(extends = Element, subclass, module = "gedcom.element.root")]
pub struct RootElement;

#[pymethods]
impl RootElement {
    #[new]
    #[pyo3(signature = (
        level = -1,
        pointer = Some(""),
        tag = "ROOT",
        value = "",
        crlf = "\n",
        multi_line = true
    ))]
    fn py_new(
        py: Python<'_>,
        level: i64,
        pointer: Option<&str>,
        tag: &str,
        value: &str,
        crlf: &str,
        multi_line: bool,
    ) -> PyResult<PyClassInitializer<Self>> {
        let element = Element::create(
            py,
            level,
            pointer.map(str::to_owned),
            tag.to_owned(),
            value.to_owned(),
            crlf.to_owned(),
            Kind::Root,
        )?;
        if multi_line {
            element.apply_multi_line_value(py, None, value)?;
        }
        Ok(PyClassInitializer::from(element).add_subclass(RootElement))
    }
}

pyo3::create_exception!(
    gedcom.element.file,
    NotAnActualFileError,
    pyo3::exceptions::PyException
);

pyo3::create_exception!(
    gedcom.element.object,
    NotAnActualObjectError,
    pyo3::exceptions::PyException
);
