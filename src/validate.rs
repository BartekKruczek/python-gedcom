//! Checking a parsed document against the GEDCOM release it claims to be.

use std::collections::{HashMap, HashSet};

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::arena::ArenaData;

/// One thing wrong with a document.
#[pyclass(frozen, module = "gedcom.parser")]
pub struct Finding {
    /// The 1-based line the finding is about, or `0` for the document as a whole
    ///
    /// :rtype: int
    #[pyo3(get)]
    line_number: usize,
    /// A stable identifier for the rule, such as `"tag-not-in-version"`
    ///
    /// :rtype: str
    #[pyo3(get)]
    rule: String,
    /// What is wrong, in plain words
    ///
    /// :rtype: str
    #[pyo3(get)]
    message: String,
    /// `"error"` for a violation, `"warning"` for something merely suspect
    ///
    /// :rtype: str
    #[pyo3(get)]
    severity: String,
    /// The tag of the offending line, if the finding is about one
    ///
    /// :rtype: str or None
    #[pyo3(get)]
    tag: Option<String>,
}

#[pymethods]
impl Finding {
    fn __repr__(&self) -> String {
        format!(
            "Finding(line_number={}, tag={}, rule={:?}, severity={:?})",
            self.line_number,
            match &self.tag {
                Some(tag) => format!("{:?}", tag),
                None => "None".to_owned(),
            },
            self.rule,
            self.severity
        )
    }
}

impl Finding {
    fn about(line_number: usize, tag: &str, rule: &str, severity: &str, message: String) -> Finding {
        Finding {
            line_number,
            rule: rule.to_owned(),
            message,
            severity: severity.to_owned(),
            tag: Some(tag.to_owned()),
        }
    }

    fn document(rule: &str, severity: &str, message: String) -> Finding {
        Finding {
            line_number: 0,
            rule: rule.to_owned(),
            message,
            severity: severity.to_owned(),
            tag: None,
        }
    }
}

/// What `gedcom.config.versions` says about one release.
struct Rules {
    version: Option<String>,
    introduced_later: HashSet<String>,
    removed_by: HashSet<String>,
    xref_minimum: usize,
    line_limit: usize,
}

impl Rules {
    /// Read the rules for `version` out of `gedcom.config.versions`.
    ///
    /// A release the module records nothing about -- 7.0, or a typo -- yields
    /// rules that judge nothing beyond what every release shares.
    fn load(py: Python<'_>, version: Option<&str>) -> PyResult<Rules> {
        let module = py.import("gedcom.config.versions")?;
        let known: Vec<String> = module.getattr("VERSIONS")?.extract()?;
        let recorded = version.filter(|name| known.iter().any(|v| v == name));

        let mut introduced_later = HashSet::new();
        let mut removed_by = HashSet::new();

        if let Some(name) = recorded {
            let order = |v: &str| known.iter().position(|k| k == v);
            let current = order(name);

            let introduced: HashMap<String, HashSet<String>> =
                module.getattr("TAGS_INTRODUCED")?.extract()?;
            for (release, tags) in introduced {
                // A tag that arrives after the declared release cannot be in
                // a file of that release.
                if order(&release) > current {
                    introduced_later.extend(tags);
                }
            }

            let removed: HashMap<String, HashSet<String>> =
                module.getattr("TAGS_REMOVED")?.extract()?;
            for (release, tags) in removed {
                if order(&release) <= current {
                    removed_by.extend(tags);
                }
            }
        }

        let minimums: HashMap<String, usize> = module.getattr("XREF_MINIMUM")?.extract()?;
        let xref_minimum = recorded
            .and_then(|name| minimums.get(name).copied())
            .unwrap_or(1);

        let line_limit: usize = module
            .call_method1("line_limit", (version,))?
            .extract()?;

        Ok(Rules {
            version: version.map(str::to_owned),
            introduced_later,
            removed_by,
            xref_minimum,
            line_limit,
        })
    }
}

/// Find `HEAD`'s `GEDC.VERS`, which is what a document says it is.
fn declared_version(data: &ArenaData, root: u32) -> Option<String> {
    let head = data
        .children_of(root)?
        .find(|id| data.tag_of(*id).eq_ignore_ascii_case("HEAD"))?;
    let gedc = data
        .children_of(head)?
        .find(|id| data.tag_of(*id).eq_ignore_ascii_case("GEDC"))?;
    let vers = data
        .children_of(gedc)?
        .find(|id| data.tag_of(*id).eq_ignore_ascii_case("VERS"))?;
    let value = data.value_of(vers).trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

/// Find `HEAD.CHAR`.
fn declared_character_set(data: &ArenaData, root: u32) -> Option<String> {
    let head = data
        .children_of(root)?
        .find(|id| data.tag_of(*id).eq_ignore_ascii_case("HEAD"))?;
    let char_line = data
        .children_of(head)?
        .find(|id| data.tag_of(*id).eq_ignore_ascii_case("CHAR"))?;
    let value = data.value_of(char_line).trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

/// Whether a value is a cross-reference rather than text.
fn is_pointer(value: &str) -> bool {
    value.len() >= 2 && value.starts_with('@') && value.ends_with('@') && !value[1..].starts_with('#')
}

/// Every `@` in a text value has to be doubled; one that is not would be read
/// as the start of a pointer or an escape.
fn has_lone_at_sign(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'@' {
            if index + 1 < bytes.len() && bytes[index + 1] == b'@' {
                index += 2;
                continue;
            }
            return true;
        }
        index += 1;
    }
    false
}

/// Check a parsed document, and report everything that does not hold.
///
/// The nodes are visited once in document order; line numbers are their
/// position in that order, which is what the parser assigned them.
pub fn run(
    py: Python<'_>,
    data: &ArenaData,
    root: u32,
    requested: Option<&str>,
    unterminated_final_line: bool,
) -> PyResult<Py<PyList>> {
    let version = requested
        .map(str::to_owned)
        .or_else(|| declared_version(data, root));
    let rules = Rules::load(py, version.as_deref())?;

    let mut findings: Vec<Finding> = Vec::new();

    // The character set is the one header line whose legality depends on the
    // release, and `gedcom.config.versions` already words that finding.
    if let Some(character_set) = declared_character_set(data, root) {
        let note: Option<String> = py
            .import("gedcom.config.versions")?
            .call_method1("version_note", (rules.version.clone(), character_set))?
            .extract()?;
        if let Some(message) = note {
            findings.push(Finding::document("character-set", "error", message));
        }
    }

    if unterminated_final_line {
        findings.push(Finding::document(
            "final-terminator-missing",
            "error",
            "the last line carries no terminator, which GEDCOM requires of \
             every line; it was read anyway because the writing program is \
             known to omit it"
                .to_owned(),
        ));
    }

    let records: Vec<u32> = match data.children_of(root) {
        Some(children) => children.collect(),
        None => Vec::new(),
    };

    match records.first() {
        None => findings.push(Finding::document(
            "header-missing",
            "error",
            "the document is empty".to_owned(),
        )),
        Some(first) if !data.tag_of(*first).eq_ignore_ascii_case("HEAD") => {
            findings.push(Finding::document(
                "header-missing",
                "error",
                format!(
                    "the first record is {}, but every GEDCOM file begins with HEAD",
                    data.tag_of(*first)
                ),
            ))
        }
        Some(_) => {}
    }

    if let Some(last) = records.last() {
        if !data.tag_of(*last).eq_ignore_ascii_case("TRLR") {
            findings.push(Finding::document(
                "trailer-missing",
                "error",
                format!(
                    "the last record is {}, but every GEDCOM file ends with TRLR",
                    data.tag_of(*last)
                ),
            ));
        }
    }

    // Pointers are collected on the way through and resolved at the end, so a
    // reference to a record defined later is not reported as dangling.
    let mut defined: HashMap<String, usize> = HashMap::new();
    let mut referenced: Vec<(usize, String, String)> = Vec::new();

    for (index, node) in data.nodes.iter().enumerate() {
        let id = index as u32;
        if id == root {
            continue;
        }
        let line_number = index;
        let tag = data.tag_of(id).to_owned();
        let upper = tag.to_ascii_uppercase();

        if rules.introduced_later.contains(&upper) {
            findings.push(Finding::about(
                line_number,
                &tag,
                "tag-not-in-version",
                "error",
                format!(
                    "{} did not exist in GEDCOM {}",
                    tag,
                    rules.version.as_deref().unwrap_or("?")
                ),
            ));
        }

        if rules.removed_by.contains(&upper) {
            findings.push(Finding::about(
                line_number,
                &tag,
                "tag-removed-in-version",
                "error",
                format!(
                    "{} was dropped in GEDCOM {}",
                    tag,
                    rules.version.as_deref().unwrap_or("?")
                ),
            ));
        }

        if node.level == 0 && (upper == "CONC" || upper == "CONT") {
            findings.push(Finding::about(
                line_number,
                &tag,
                "continuation-at-level-0",
                "error",
                format!("{} continues a value, so it cannot be a record", tag),
            ));
        }

        // A line with no pointer reads back as an empty one, which is not
        // the same as defining a cross-reference called "".
        if let Some(pointer) = data.pointer_of(id).filter(|p| !p.is_empty()) {
            let name = pointer.trim_matches('@');
            if name.chars().count() < rules.xref_minimum {
                findings.push(Finding::about(
                    line_number,
                    &tag,
                    "xref-too-short",
                    "error",
                    format!(
                        "the identifier {} is shorter than the {} characters GEDCOM {} requires",
                        pointer,
                        rules.xref_minimum,
                        rules.version.as_deref().unwrap_or("?")
                    ),
                ));
            }
            if let Some(first) = defined.insert(pointer.to_owned(), line_number) {
                findings.push(Finding::about(
                    line_number,
                    &tag,
                    "xref-duplicate",
                    "error",
                    format!("{} was already defined on line {}", pointer, first),
                ));
            }
        }

        let value = data.value_of(id);
        if is_pointer(value) {
            referenced.push((line_number, tag.clone(), value.to_owned()));
        } else if has_lone_at_sign(value) {
            findings.push(Finding::about(
                line_number,
                &tag,
                "at-sign-not-doubled",
                "warning",
                "an @ in a value has to be written @@; this one is not, so a \
                 conforming reader would take it for a pointer"
                    .to_owned(),
            ));
        }

        // GEDCOM counts the terminator toward the limit, so it is measured
        // here rather than assumed: a file may end its lines with CR, LF or
        // both.
        let rendered = node.level.to_string().len()
            + 1
            + data
                .pointer_of(id)
                .filter(|p| !p.is_empty())
                .map_or(0, |p| p.chars().count() + 1)
            + tag.chars().count()
            + if value.is_empty() { 0 } else { value.chars().count() + 1 }
            + data.crlf_of(id).chars().count();
        if rendered > rules.line_limit {
            findings.push(Finding::about(
                line_number,
                &tag,
                "line-too-long",
                "error",
                format!(
                    "the line is {} characters, over the {} GEDCOM {} allows",
                    rendered,
                    rules.line_limit,
                    rules.version.as_deref().unwrap_or("?")
                ),
            ));
        }
    }

    for (line_number, tag, pointer) in referenced {
        if !defined.contains_key(&pointer) {
            findings.push(Finding::about(
                line_number,
                &tag,
                "pointer-unresolved",
                "error",
                format!("{} does not name any record in this document", pointer),
            ));
        }
    }

    findings.sort_by_key(|f| (f.line_number, f.rule.clone()));

    let list = PyList::empty(py);
    for finding in findings {
        list.append(Py::new(py, finding)?)?;
    }
    Ok(list.unbind())
}

/// `_validation_rules()` -- what the core read for a version, for testing.
pub fn rules_for<'py>(py: Python<'py>, version: Option<&str>) -> PyResult<Bound<'py, PyDict>> {
    let rules = Rules::load(py, version)?;
    let out = PyDict::new(py);
    out.set_item("xref_minimum", rules.xref_minimum)?;
    out.set_item("line_limit", rules.line_limit)?;
    let mut later: Vec<String> = rules.introduced_later.into_iter().collect();
    later.sort();
    out.set_item("introduced_later", later)?;
    let mut removed: Vec<String> = rules.removed_by.into_iter().collect();
    removed.sort();
    out.set_item("removed_by", removed)?;
    Ok(out)
}
