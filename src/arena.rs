//! Node storage.

use std::collections::HashMap;

use parking_lot::RwLock;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyList;
use pyo3::PyTraverseError;
use pyo3::PyVisit;

use crate::tags;

pub const NONE: u32 = u32::MAX;

const FOREIGN: u32 = u32::MAX - 1;

const MATERIALIZED: u8 = 1;

pub const TEXT_LIMIT: usize = u32::MAX as usize - 1;

const PREAMBLE: &str = "\nCONC";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Base,
    Individual,
    Family,
    File,
    Object,
    Root,
}

impl Kind {
    pub fn for_tag(tag: &str) -> Kind {
        match tag {
            tags::INDIVIDUAL => Kind::Individual,
            tags::FAMILY => Kind::Family,
            tags::FILE => Kind::File,
            tags::OBJECT => Kind::Object,
            _ => Kind::Base,
        }
    }

    pub fn overridden_tag(self) -> Option<&'static str> {
        match self {
            Kind::Individual => Some(tags::INDIVIDUAL),
            Kind::Family => Some(tags::FAMILY),
            Kind::File => Some(tags::FILE),
            // `ObjectElement` and `RootElement` do not override `get_tag()`.
            Kind::Base | Kind::Object | Kind::Root => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Text {
    start: u32,
    len: u32,
}

impl Text {
    pub const ABSENT: Text = Text {
        start: NONE,
        len: NONE,
    };

    pub const EMPTY: Text = Text { start: 0, len: 0 };

    pub const LINE_FEED: Text = Text { start: 0, len: 1 };

    pub const CONC: Text = Text { start: 1, len: 4 };

    pub fn is_absent(self) -> bool {
        self.start == NONE && self.len == NONE
    }
}

pub struct Node {
    pub level: i64,
    pointer: Text,
    tag: Text,
    value: Text,
    crlf: Text,
    first_child: u32,
    last_child: u32,
    child_count: u32,
    next_sibling: u32,
    parent: u32,
    pub kind: Kind,
    flags: u8,
}

impl Node {
    fn new(level: i64, pointer: Text, tag: Text, value: Text, crlf: Text, kind: Kind) -> Node {
        Node {
            level,
            pointer,
            tag,
            value,
            crlf,
            first_child: NONE,
            last_child: NONE,
            child_count: 0,
            next_sibling: NONE,
            parent: NONE,
            kind,
            flags: 0,
        }
    }

    pub fn is_materialized(&self) -> bool {
        self.flags & MATERIALIZED != 0
    }

    pub fn child_count(&self) -> u32 {
        self.child_count
    }
}

pub struct ArenaData {
    pub nodes: Vec<Node>,
    text: String,
    owned: Vec<Box<str>>,
    pub handles: Vec<Option<Py<PyAny>>>,
    materialized: Vec<Option<Py<PyList>>>,
    foreign_parents: HashMap<u32, Py<PyAny>>,
    line_limit: usize,
}

pub const DEFAULT_LINE_LIMIT: usize = 255;

impl ArenaData {
    pub fn line_limit(&self) -> usize {
        self.line_limit
    }

    pub fn set_line_limit(&mut self, limit: usize) {
        self.line_limit = limit;
    }

    pub fn node(&self, id: u32) -> &Node {
        &self.nodes[id as usize]
    }

    fn node_mut(&mut self, id: u32) -> &mut Node {
        &mut self.nodes[id as usize]
    }

    pub fn text_of(&self, text: Text) -> Option<&str> {
        if text.is_absent() {
            return None;
        }
        if text.start == NONE {
            return Some(&self.owned[text.len as usize]);
        }
        Some(&self.text[text.start as usize..(text.start + text.len) as usize])
    }

    pub fn str_of(&self, text: Text) -> &str {
        self.text_of(text).unwrap_or("")
    }

    pub fn intern(&mut self, value: &str) -> PyResult<Text> {
        if value.is_empty() {
            return Ok(Text::EMPTY);
        }
        let start = self.text.len();
        if start + value.len() > TEXT_LIMIT {
            return Err(text_limit_error());
        }
        self.text.push_str(value);
        Ok(Text {
            start: start as u32,
            len: value.len() as u32,
        })
    }

    pub fn adopt_text(&mut self, source: &str) -> PyResult<u32> {
        let base = self.text.len();
        if base + source.len() > TEXT_LIMIT {
            return Err(text_limit_error());
        }
        self.text.push_str(source);
        Ok(base as u32)
    }

    pub fn span(&self, start: usize, len: usize) -> Text {
        debug_assert!(
            start + len <= self.text.len(),
            "span {}..{} outside a text buffer of {}",
            start,
            start + len,
            self.text.len()
        );
        Text {
            start: start as u32,
            len: len as u32,
        }
    }

    pub fn text_slice(&self, start: usize, len: usize) -> &str {
        &self.text[start..start + len]
    }

    pub fn text_len(&self) -> usize {
        self.text.len()
    }

    pub fn tag_text_of(&self, id: u32) -> Text {
        self.node(id).tag
    }

    pub fn set_text(&mut self, slot: &mut Text, value: &str) -> PyResult<()> {
        if slot.start == NONE && !slot.is_absent() {
            self.owned[slot.len as usize] = value.into();
            return Ok(());
        }
        if self.owned.len() >= NONE as usize {
            return Err(text_limit_error());
        }
        self.owned.push(value.into());
        *slot = Text {
            start: NONE,
            len: (self.owned.len() - 1) as u32,
        };
        Ok(())
    }

    pub fn pointer_of(&self, id: u32) -> Option<&str> {
        self.text_of(self.node(id).pointer)
    }

    pub fn tag_of(&self, id: u32) -> &str {
        self.str_of(self.node(id).tag)
    }

    pub fn effective_tag_of(&self, id: u32) -> &str {
        let node = self.node(id);
        node.kind.overridden_tag().unwrap_or(self.str_of(node.tag))
    }

    pub fn value_of(&self, id: u32) -> &str {
        self.str_of(self.node(id).value)
    }

    pub fn crlf_of(&self, id: u32) -> &str {
        self.str_of(self.node(id).crlf)
    }

    pub fn set_value_of(&mut self, id: u32, value: &str) -> PyResult<()> {
        let mut slot = self.node(id).value;
        self.set_text(&mut slot, value)?;
        self.node_mut(id).value = slot;
        Ok(())
    }

    pub fn push(
        &mut self,
        level: i64,
        pointer: Text,
        tag: Text,
        value: Text,
        crlf: Text,
        kind: Kind,
    ) -> u32 {
        let id = self.nodes.len() as u32;
        self.nodes
            .push(Node::new(level, pointer, tag, value, crlf, kind));
        self.handles.push(None);
        id
    }

    pub fn push_owned(
        &mut self,
        level: i64,
        pointer: Option<&str>,
        tag: &str,
        value: &str,
        crlf: &str,
        kind: Kind,
    ) -> PyResult<u32> {
        let pointer = match pointer {
            Some(text) => self.intern(text)?,
            None => Text::ABSENT,
        };
        let tag = self.intern(tag)?;
        let value = self.intern(value)?;
        let crlf = self.intern(crlf)?;
        Ok(self.push(level, pointer, tag, value, crlf, kind))
    }

    pub fn children_of(&self, id: u32) -> Option<ChildIter<'_>> {
        let node = self.node(id);
        if node.is_materialized() {
            return None;
        }
        Some(ChildIter {
            data: self,
            next: node.first_child,
        })
    }

    pub fn append_child(&mut self, parent: u32, child: u32) {
        self.node_mut(child).next_sibling = NONE;
        let last = self.node(parent).last_child;
        if last == NONE {
            self.node_mut(parent).first_child = child;
        } else {
            self.node_mut(last).next_sibling = child;
        }
        let node = self.node_mut(parent);
        node.last_child = child;
        node.child_count += 1;
    }

    pub fn retain_children(&mut self, parent: u32, keep: impl Fn(&ArenaData, u32) -> bool) {
        let mut kept: Vec<u32> = Vec::new();
        let mut current = self.node(parent).first_child;
        while current != NONE {
            if keep(self, current) {
                kept.push(current);
            }
            current = self.node(current).next_sibling;
        }

        for window in kept.windows(2) {
            self.node_mut(window[0]).next_sibling = window[1];
        }
        if let Some(&last) = kept.last() {
            self.node_mut(last).next_sibling = NONE;
        }

        let first = kept.first().copied().unwrap_or(NONE);
        let last = kept.last().copied().unwrap_or(NONE);
        let count = kept.len() as u32;
        let node = self.node_mut(parent);
        node.first_child = first;
        node.last_child = last;
        node.child_count = count;
    }

    pub fn parent_of(&self, id: u32) -> Option<u32> {
        match self.node(id).parent {
            NONE | FOREIGN => None,
            parent => Some(parent),
        }
    }

    pub fn foreign_parent_of(&self, id: u32) -> Option<&Py<PyAny>> {
        if self.node(id).parent != FOREIGN {
            return None;
        }
        self.foreign_parents.get(&id)
    }

    pub fn set_local_parent(&mut self, id: u32, parent: u32) {
        self.foreign_parents.remove(&id);
        self.node_mut(id).parent = parent;
    }

    pub fn clear_parent(&mut self, id: u32) {
        self.foreign_parents.remove(&id);
        self.node_mut(id).parent = NONE;
    }

    pub fn set_foreign_parent(&mut self, id: u32, parent: Py<PyAny>) {
        self.node_mut(id).parent = FOREIGN;
        self.foreign_parents.insert(id, parent);
    }

    pub fn materialized_list(&self, id: u32) -> Option<&Py<PyList>> {
        self.materialized.get(id as usize)?.as_ref()
    }

    pub fn set_materialized_list(&mut self, id: u32, list: Py<PyList>) {
        if self.materialized.len() <= id as usize {
            self.materialized.resize_with(self.nodes.len(), || None);
        }
        self.materialized[id as usize] = Some(list);
        self.node_mut(id).flags |= MATERIALIZED;
    }

    pub fn subtree_metrics(&self, id: u32) -> Option<usize> {
        let mut total = 0usize;
        let mut stack = vec![id];
        while let Some(current) = stack.pop() {
            let node = self.node(current);
            if node.is_materialized() {
                return None;
            }
            total += 24
                + self.str_of(node.pointer).len()
                + self.str_of(node.tag).len()
                + self.str_of(node.value).len()
                + self.str_of(node.crlf).len();
            let mut child = node.first_child;
            while child != NONE {
                stack.push(child);
                child = self.node(child).next_sibling;
            }
        }
        Some(total)
    }
}

fn text_limit_error() -> PyErr {
    PyValueError::new_err(
        "GEDCOM document exceeds the 4 GiB the parser can address in one tree",
    )
}

pub struct ChildIter<'a> {
    data: &'a ArenaData,
    next: u32,
}

impl Iterator for ChildIter<'_> {
    type Item = u32;

    fn next(&mut self) -> Option<u32> {
        if self.next == NONE {
            return None;
        }
        let current = self.next;
        self.next = self.data.node(current).next_sibling;
        Some(current)
    }
}

#[pyclass(frozen, module = "gedcom._gedcom")]
pub struct Arena {
    data: RwLock<ArenaData>,
}

impl Arena {
    pub fn empty() -> Arena {
        Arena::with_capacity(0)
    }

    pub fn with_capacity(capacity: usize) -> Arena {
        Arena {
            data: RwLock::new(ArenaData {
                nodes: Vec::with_capacity(capacity),
                text: String::from(PREAMBLE),
                owned: Vec::new(),
                handles: Vec::with_capacity(capacity),
                materialized: Vec::new(),
                foreign_parents: HashMap::new(),
                line_limit: DEFAULT_LINE_LIMIT,
            }),
        }
    }

    pub fn read(&self) -> parking_lot::RwLockReadGuard<'_, ArenaData> {
        self.data.read()
    }

    pub fn write(&self) -> parking_lot::RwLockWriteGuard<'_, ArenaData> {
        self.data.write()
    }

    pub fn cached_handle(&self, py: Python<'_>, id: u32) -> Option<Py<PyAny>> {
        let data = self.data.read();
        Some(data.handles.get(id as usize)?.as_ref()?.clone_ref(py))
    }

    pub fn remember_handle(&self, id: u32, handle: &Bound<'_, PyAny>) -> PyResult<()> {
        let mut data = self.data.write();
        if let Some(slot) = data.handles.get_mut(id as usize) {
            *slot = Some(handle.clone().unbind());
        }
        Ok(())
    }
}

#[pymethods]
impl Arena {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        let Some(data) = self.data.try_read() else {
            return Ok(());
        };
        for list in data.materialized.iter().flatten() {
            visit.call(list)?;
        }
        for parent in data.foreign_parents.values() {
            visit.call(parent)?;
        }
        for handle in data.handles.iter().flatten() {
            visit.call(handle)?;
        }
        Ok(())
    }

    fn __clear__(&self) {
        let Some(mut data) = self.data.try_write() else {
            return;
        };
        data.materialized.clear();
        data.foreign_parents.clear();
        data.handles.clear();
        for node in data.nodes.iter_mut() {
            node.flags = 0;
            node.parent = NONE;
        }
    }
}
