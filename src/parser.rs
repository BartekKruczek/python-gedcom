//! `Parser` -- reading GEDCOM data and analysing the tree it produces.

use parking_lot::RwLockWriteGuard;
use pyo3::exceptions::PyAttributeError;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use pyo3::types::PyDict;
use pyo3::types::PyList;
use pyo3::types::PyTuple;

use crate::arena::{Arena, ArenaData, Kind, Text, DEFAULT_LINE_LIMIT};
use crate::element::{self, children_of, make_handle, with_children, Child, Element};
use crate::individual::{require_individual, IndividualElement};
use crate::pystr;
use crate::scanner;
use crate::tags;

pyo3::create_exception!(
    gedcom.parser,
    GedcomFormatViolationError,
    pyo3::exceptions::PyException
);

pyo3::create_exception!(
    gedcom.element.family,
    NotAnActualFamilyError,
    pyo3::exceptions::PyException
);

const SPEC_URL: &str = "https://chronoplexsoftware.com/gedcomvalidator/gedcom/gedcom-5.5.pdf";

/// How a document should be decoded.
enum Encoding {
    Utf8,
    Other(String),
}

/// The parser's settings, unpacked from a `gedcom.config.ParserConfig`.
struct Options {
    strict: bool,
    encoding: Encoding,
    collect: bool,
    load_from_source: bool,
}

fn encoding_from_name(name: &str) -> Encoding {
    match name {
        "utf-8-sig" | "utf_8_sig" | "utf-8" | "utf8" | "utf_8" => Encoding::Utf8,
        _ => Encoding::Other(name.to_owned()),
    }
}

impl Options {
    /// Read the settings off a config object.
    ///
    /// Attributes are read by name rather than by type, so any object shaped
    /// like `ParserConfig` -- including a user's own model -- works.
    fn from_config(config: &Bound<'_, PyAny>) -> PyResult<Options> {
        let encoding: String = config.getattr("encoding")?.extract()?;
        let on_error: String = config.getattr("on_error")?.extract()?;

        if on_error != "raise" && on_error != "collect" {
            return Err(PyValueError::new_err(format!(
                "on_error must be \"raise\" or \"collect\", not {:?}",
                on_error
            )));
        }

        Ok(Options {
            strict: config.getattr("strict")?.extract()?,
            encoding: encoding_from_name(&encoding),
            collect: on_error == "collect",
            // A config object shaped like an older `ParserConfig` simply does
            // not have the feature; that is not the same as a default living
            // here as well as in the model.
            load_from_source: match config.getattr("load_from_source") {
                Ok(value) => value.extract()?,
                Err(_) => false,
            },
        })
    }
}

/// The default settings, built once per process from `ParserConfig()`.
fn default_options(py: Python<'_>) -> PyResult<&'static Options> {
    static DEFAULTS: pyo3::sync::PyOnceLock<Options> = pyo3::sync::PyOnceLock::new();
    DEFAULTS.get_or_try_init(py, || {
        let config = py
            .import("gedcom.config")?
            .getattr("ParserConfig")?
            .call0()?;
        Options::from_config(&config)
    })
}

/// One line the parser could not accept, kept when `on_error` is `"collect"`.
#[pyclass(frozen, module = "gedcom.parser")]
pub struct ParseError {
    /// The 1-based number of the offending line
    ///
    /// :rtype: int
    #[pyo3(get)]
    line_number: usize,
    /// The offending line, terminator included
    ///
    /// :rtype: str
    #[pyo3(get)]
    line: String,
    /// The message the parser would have raised
    ///
    /// :rtype: str
    #[pyo3(get)]
    message: String,
    /// The name of the exception class that would have been raised
    ///
    /// :rtype: str
    #[pyo3(get)]
    error_type: &'static str,
}

#[pymethods]
impl ParseError {
    fn __repr__(&self) -> String {
        format!(
            "ParseError(line_number={}, error_type={:?}, message={:?})",
            self.line_number, self.error_type, self.message
        )
    }
}

/// What the document's own header said about itself.
///
/// Present on `Parser.source` after a parse with `load_from_source` set, and
/// `None` otherwise.
#[pyclass(frozen, module = "gedcom.parser")]
pub struct SourceInfo {
    /// The `HEAD.SOUR` value, verbatim
    ///
    /// :rtype: str or None
    #[pyo3(get)]
    system: Option<String>,
    /// The program's name, if it is one `gedcom.config.sources` knows
    ///
    /// :rtype: str or None
    #[pyo3(get)]
    name: Option<String>,
    /// The `HEAD.SOUR.VERS` value -- the version of the program, not of GEDCOM
    ///
    /// :rtype: str or None
    #[pyo3(get)]
    version: Option<String>,
    /// The `HEAD.GEDC.VERS` value -- which GEDCOM release the file claims
    ///
    /// :rtype: str or None
    #[pyo3(get)]
    gedcom_version: Option<String>,
    /// The `HEAD.CHAR` value, verbatim
    ///
    /// :rtype: str or None
    #[pyo3(get)]
    character_set: Option<String>,
    /// The codec the document was actually read with
    ///
    /// :rtype: str
    #[pyo3(get)]
    encoding: String,
    /// Whether the final line was allowed to carry no terminator
    ///
    /// :rtype: bool
    #[pyo3(get)]
    unterminated_final_line: bool,
    /// What the header said that does not add up, in plain words
    ///
    /// Never fatal: a mislabelled header is ordinary, and the file still reads.
    ///
    /// :rtype: list of str
    #[pyo3(get)]
    notes: Vec<String>,
}

#[pymethods]
impl SourceInfo {
    fn __repr__(&self) -> String {
        // `{:?}` on an `Option` would render Rust's own `Some(..)`/`None`.
        let quoted = |value: &Option<String>| match value {
            Some(text) => format!("{:?}", text),
            None => "None".to_owned(),
        };
        format!(
            "SourceInfo(system={}, name={}, version={}, gedcom_version={}, character_set={}, encoding={:?}, unterminated_final_line={}, notes={})",
            quoted(&self.system),
            quoted(&self.name),
            quoted(&self.version),
            quoted(&self.gedcom_version),
            quoted(&self.character_set),
            self.encoding,
            if self.unterminated_final_line { "True" } else { "False" },
            self.notes.len(),
        )
    }
}

/// The lines of the `HEAD` record that say how to read the rest.
#[derive(Default)]
struct HeaderScan {
    character_set: Option<String>,
    system: Option<String>,
    version: Option<String>,
    gedcom_version: Option<String>,
    destination: Option<String>,
}

/// Read the `HEAD` record, and stop at the record after it.
///
/// Deliberately tolerant: this runs before the encoding is known, so it works
/// on a lossy view of the bytes. Every value it looks for is ASCII in practice.
fn scan_header(text: &str) -> HeaderScan {
    let mut found = HeaderScan::default();
    let mut parent = String::new();

    for (index, line) in text.lines().enumerate() {
        let line = line.trim_end_matches(['\r', '\n']);
        if let Some(rest) = line.strip_prefix("0 ") {
            // The header is the first record; the next level 0 line ends it.
            if index > 0 || !rest.trim().eq_ignore_ascii_case("HEAD") {
                break;
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix("1 ") {
            let (tag, value) = split_tag(rest);
            parent = tag.to_ascii_uppercase();
            match parent.as_str() {
                "CHAR" => found.character_set = value,
                "SOUR" => found.system = value,
                "DEST" => found.destination = value,
                _ => {}
            }
        } else if let Some(rest) = line.strip_prefix("2 ") {
            let (tag, value) = split_tag(rest);
            if tag.eq_ignore_ascii_case("VERS") {
                // `2 VERS` is the program's version under `SOUR` and the
                // document's own under `GEDC`.
                match parent.as_str() {
                    "SOUR" => found.version = value,
                    "GEDC" => found.gedcom_version = value,
                    _ => {}
                }
            }
        }
    }

    found
}

/// Split `TAG value` into its two halves, with an absent or empty value as
/// `None`.
fn split_tag(rest: &str) -> (&str, Option<String>) {
    match rest.find(' ') {
        Some(cut) => {
            let value = rest[cut + 1..].trim();
            (
                &rest[..cut],
                if value.is_empty() {
                    None
                } else {
                    Some(value.to_owned())
                },
            )
        }
        None => (rest.trim(), None),
    }
}

/// A view of the first bytes of a document, good enough to read its header.
///
/// UTF-16 is the one encoding whose header is not ASCII bytes, and its BOM
/// says so; everything else survives a lossy UTF-8 read intact, because the
/// values being looked for are ASCII.
fn header_text(py: Python<'_>, buffer: &[u8]) -> PyResult<String> {
    const WINDOW: usize = 8192;
    let head = &buffer[..buffer.len().min(WINDOW)];

    if head.starts_with(&[0xFF, 0xFE]) || head.starts_with(&[0xFE, 0xFF]) {
        let kwargs = PyDict::new(py);
        kwargs.set_item("errors", "ignore")?;
        return PyBytes::new(py, head)
            .call_method("decode", ("utf-16",), Some(&kwargs))?
            .extract();
    }

    Ok(scanner::strip_bom(&String::from_utf8_lossy(head)).to_owned())
}

/// Turn a header into the settings it calls for.
///
/// The character set and the profile both live in `gedcom.config.sources`; nothing is
/// decided here beyond which of them applies.
fn resolve_source(
    py: Python<'_>,
    found: &HeaderScan,
    configured: &Encoding,
) -> PyResult<(Encoding, bool, usize, SourceInfo)> {
    let sources = py.import("gedcom.config.sources")?;

    let declared: Option<String> = sources
        .call_method1("encoding_for", (found.character_set.clone(),))?
        .extract()?;

    let encoding = match &declared {
        Some(name) => encoding_from_name(name),
        None => match configured {
            Encoding::Utf8 => Encoding::Utf8,
            Encoding::Other(name) => Encoding::Other(name.clone()),
        },
    };

    // `HEAD.DEST` names the dialect a file was written for, which is what
    // matters when `HEAD.SOUR` is a program nothing is recorded about.
    let profile = {
        let by_system = sources.call_method1("profile_for", (found.system.clone(),))?;
        if by_system.is_none() {
            sources.call_method1("profile_for", (found.destination.clone(),))?
        } else {
            by_system
        }
    };

    let (name, unterminated) = if profile.is_none() {
        (None, false)
    } else {
        (
            Some(profile.getattr("name")?.extract()?),
            profile.getattr("unterminated_final_line")?.extract()?,
        )
    };

    let versions = py.import("gedcom.config.versions")?;
    let note: Option<String> = versions
        .call_method1(
            "version_note",
            (found.gedcom_version.clone(), found.character_set.clone()),
        )?
        .extract()?;

    let info = SourceInfo {
        system: found.system.clone(),
        name,
        version: found.version.clone(),
        gedcom_version: found.gedcom_version.clone(),
        character_set: found.character_set.clone(),
        notes: note.into_iter().collect(),
        encoding: match &encoding {
            Encoding::Utf8 => "utf-8-sig".to_owned(),
            Encoding::Other(name) => name.clone(),
        },
        unterminated_final_line: unterminated,
    };

    let limit: usize = versions
        .call_method1("line_limit", (found.gedcom_version.clone(),))?
        .extract()?;

    Ok((encoding, unterminated, limit, info))
}

enum ParseFault {
    Format(String),
    Attribute(String),
    Decode(usize),
}

impl ParseFault {
    fn error_type(&self) -> &'static str {
        match self {
            ParseFault::Format(_) => "GedcomFormatViolationError",
            ParseFault::Attribute(_) => "AttributeError",
            ParseFault::Decode(_) => "UnicodeDecodeError",
        }
    }

    fn message(&self) -> String {
        match self {
            ParseFault::Format(message) | ParseFault::Attribute(message) => message.clone(),
            ParseFault::Decode(_) => "could not be decoded".to_owned(),
        }
    }
}

impl ParseFault {
    fn into_error(
        self,
        py: Python<'_>,
        buffer: Option<&[u8]>,
        lines: &[(usize, usize)],
    ) -> PyErr {
        match self {
            ParseFault::Format(message) => GedcomFormatViolationError::new_err(message),
            ParseFault::Attribute(message) => PyAttributeError::new_err(message),
            ParseFault::Decode(index) => match buffer {
                Some(buffer) => {
                    let (start, len) = lines[index];
                    decode_error(py, &buffer[start..start + len], "utf-8-sig")
                }
                None => PyAttributeError::new_err("decoding failed"),
            },
        }
    }
}

/// Build one `ParseError`, given the offending line's text.
fn build_error(
    py: Python<'_>,
    index: usize,
    line: &str,
    fault: &ParseFault,
) -> PyResult<Py<ParseError>> {
    Py::new(
        py,
        ParseError {
            line_number: index + 1,
            line: line.to_owned(),
            message: fault.message(),
            error_type: fault.error_type(),
        },
    )
}

/// Store the faults gathered during a parse on the parser.
fn record_faults(
    slf: &Bound<'_, Parser>,
    py: Python<'_>,
    faults: Vec<(usize, ParseFault)>,
    line_text: impl Fn(usize) -> String,
) -> PyResult<()> {
    let recorded: Vec<Py<ParseError>> = faults
        .into_iter()
        .map(|(index, fault)| build_error(py, index, &line_text(index), &fault))
        .collect::<PyResult<_>>()?;
    slf.borrow_mut().errors = recorded;
    Ok(())
}

fn decode_error(py: Python<'_>, line: &[u8], codec: &str) -> PyErr {
    match PyBytes::new(py, line).call_method1("decode", (codec,)) {
        Ok(_) => PyAttributeError::new_err("decoding unexpectedly succeeded"),
        Err(error) => error,
    }
}

fn strip_bom_range(source: &str, start: usize, len: usize) -> (usize, usize) {
    const BOM: &str = "\u{feff}";
    // A line running past the decoded prefix is one that failed to decode;
    // leave it alone so the parse loop can report it as such.
    if start + len > source.len() {
        return (start, len);
    }
    if source[start..start + len].starts_with(BOM) {
        return (start + BOM.len(), len - BOM.len());
    }
    (start, len)
}

fn offset_in(whole: &str, part: &str) -> usize {
    part.as_ptr() as usize - whole.as_ptr() as usize
}

struct LinePlan {
    level: i64,
    pointer: Text,
    tag: Text,
    value: Text,
    crlf: Text,
    kind: Kind,
}

fn parse_one_line(
    data: &mut ArenaData,
    start: usize,
    len: usize,
    line_number: usize,
    last_id: u32,
    strict: bool,
    allow_unterminated: bool,
) -> Result<u32, ParseFault> {
    let plan = plan_line(data, start, len, line_number, last_id, strict, allow_unterminated)?;

    if plan.level > data.node(last_id).level + 1 {
        return Err(ParseFault::Format(format!(
            "Line {} of document violates GEDCOM format 5.5\nLines must be no more than one level higher than previous line.\nSee: {}",
            line_number, SPEC_URL
        )));
    }

    let id = data.push(
        plan.level,
        plan.pointer,
        plan.tag,
        plan.value,
        plan.crlf,
        plan.kind,
    );

    let mut parent_id = last_id;
    while data.node(parent_id).level > plan.level - 1 {
        match data.parent_of(parent_id) {
            Some(next) => parent_id = next,
            None => {
                return Err(ParseFault::Attribute(
                    "'NoneType' object has no attribute 'get_level'".to_owned(),
                ))
            }
        }
    }

    data.set_local_parent(id, parent_id);
    data.append_child(parent_id, id);
    Ok(id)
}

fn plan_line(
    data: &ArenaData,
    start: usize,
    len: usize,
    line_number: usize,
    last_id: u32,
    strict: bool,
    allow_unterminated: bool,
) -> Result<LinePlan, ParseFault> {
    let line = data.text_slice(start, len);

    // An empty field is not necessarily a slice of the line: the scanner hands
    // back a static `""` when the optional pointer or value is absent, and
    // subtracting its address from the line's would be meaningless.
    let span = |part: &str| {
        if part.is_empty() {
            Text::EMPTY
        } else {
            data.span(start + offset_in(line, part), part.len())
        }
    };

    if let Some(scanned) = scanner::scan_line(line) {
        let tag = span(scanned.tag);
        return Ok(LinePlan {
            level: scanned.level,
            pointer: span(pystr::rstrip_spaces(scanned.pointer)),
            tag,
            value: span(scanned.value),
            crlf: span(scanned.crlf),
            kind: Kind::for_tag(scanned.tag),
        });
    }

    // A line with no terminator can only be the last one -- the splitter cuts
    // on the terminator itself -- so a source known to omit it gets this one
    // fallback, and nothing else, while still parsing strictly.
    if strict && !allow_unterminated {
        return Err(ParseFault::Format(format!(
            "Line <{}:{}> of document violates GEDCOM format 5.5\nSee: {}",
            line_number, line, SPEC_URL
        )));
    }

    if let Some(scanned) = scanner::scan_line_without_terminator(line) {
        return Ok(LinePlan {
            level: scanned.level,
            pointer: span(pystr::rstrip_spaces(scanned.pointer)),
            tag: span(scanned.tag),
            value: span(scanned.value),
            // The fallback substitutes a line feed regardless of what, if
            // anything, ended the line.
            crlf: Text::LINE_FEED,
            kind: Kind::for_tag(scanned.tag),
        });
    }

    // Quirk check: a text field containing a CR leaves a line with no level or
    // pointer. Turn it into a CONC or CONT.
    let Some((text, terminator)) = scanner::scan_continuation(line) else {
        // The original indexes into the failed match here.
        return Err(ParseFault::Attribute(
            "'NoneType' object has no attribute 'groups'".to_owned(),
        ));
    };

    let last_tag = data.effective_tag_of(last_id);
    let continues = last_tag == tags::CONTINUED || last_tag == tags::CONCATENATION;

    Ok(LinePlan {
        level: data.node(last_id).level + if continues { 0 } else { 1 },
        // No pointer at all, which is what makes `to_gedcom_string()` raise.
        pointer: Text::ABSENT,
        // Carrying on a CONC/CONT keeps its tag; anything else becomes a CONC.
        tag: if continues {
            data.tag_text_of(last_id)
        } else {
            Text::CONC
        },
        // The leading character is dropped, standing in for the space that a
        // well-formed line would have had.
        value: span(pystr::drop_first_char(text)),
        crlf: span(terminator),
        kind: Kind::Base,
    })
}

fn parse_buffer(
    data: &mut RwLockWriteGuard<'_, ArenaData>,
    lines: &[(usize, usize)],
    valid_up_to: usize,
    root_id: u32,
    strict: bool,
    collect: bool,
    allow_unterminated: bool,
    faults: &mut Vec<(usize, ParseFault)>,
) -> Result<(), ParseFault> {
    let mut last_id = root_id;

    for (index, (start, len)) in lines.iter().enumerate() {
        // A line reaching past the last valid UTF-8 byte is the one CPython
        // would have failed to decode -- and only once parsing gets to it, so
        // an earlier format error still wins.
        if start + len > valid_up_to {
            let fault = ParseFault::Decode(index);
            if !collect {
                return Err(fault);
            }
            faults.push((index, fault));
            continue;
        }

        match parse_one_line(data, *start, *len, index + 1, last_id, strict, allow_unterminated) {
            Ok(id) => last_id = id,
            // Collecting: the line contributes nothing and the parent stays
            // where it was, so the rest of the document still nests correctly.
            Err(fault) if collect => faults.push((index, fault)),
            Err(fault) => return Err(fault),
        }
    }

    Ok(())
}

/// Parses and manipulates GEDCOM 5.5 format data
///
/// For documentation of the GEDCOM 5.5 format, see: http://homepages.rootsweb.ancestry.com/~pmcbride/gedcom/55gctoc.htm
///
/// This parser reads and parses a GEDCOM file.
///
/// Elements may be accessed via:
///
/// * a `list` through `gedcom.parser.Parser.get_element_list()`
/// * a `dict` through `gedcom.parser.Parser.get_element_dictionary()`
#[pyclass(subclass, module = "gedcom.parser")]
pub struct Parser {
    arena: Py<Arena>,
    root: Py<PyAny>,
    element_list: Py<PyList>,
    element_dictionary: Py<PyDict>,
    options: Options,
    errors: Vec<Py<ParseError>>,
    source: Option<Py<SourceInfo>>,
}

/// `read_source` for a stream: the header has to be pulled off the front, so
/// the lines it occupied are handed back to be parsed like any other.
fn read_source_from_stream<'py>(
    slf: &Bound<'py, Parser>,
    items: &mut Bound<'py, pyo3::types::PyIterator>,
) -> PyResult<(Encoding, bool, usize, Vec<Bound<'py, PyAny>>)> {
    let py = slf.py();
    let (enabled, configured) = {
        let borrowed = slf.borrow();
        (
            borrowed.options.load_from_source,
            match &borrowed.options.encoding {
                Encoding::Utf8 => Encoding::Utf8,
                Encoding::Other(name) => Encoding::Other(name.clone()),
            },
        )
    };

    if !enabled {
        slf.borrow_mut().source = None;
        return Ok((configured, false, DEFAULT_LINE_LIMIT, Vec::new()));
    }

    // Enough to cover any header, and bounded so a file without one cannot
    // pull the whole stream into memory.
    const LIMIT: usize = 256;

    let mut held: Vec<Bound<'py, PyAny>> = Vec::new();
    let mut probe: Vec<u8> = Vec::new();
    let mut records = 0usize;

    for item in items.by_ref() {
        let item = item?;
        let raw: Vec<u8> = match item.cast::<PyBytes>() {
            Ok(bytes) => bytes.as_bytes().to_vec(),
            Err(_) => item.extract::<String>().unwrap_or_default().into_bytes(),
        };
        if raw.starts_with(b"0 ") {
            records += 1;
        }
        probe.extend_from_slice(&raw);
        if !probe.ends_with(b"\n") {
            probe.push(b'\n');
        }
        held.push(item);
        if records > 1 || held.len() >= LIMIT {
            break;
        }
    }

    let found = scan_header(&header_text(py, &probe)?);
    let (encoding, unterminated, limit, info) = resolve_source(py, &found, &configured)?;
    slf.borrow_mut().source = Some(Py::new(py, info)?);
    Ok((encoding, unterminated, limit, held))
}

/// Apply `load_from_source` to a parser, and report what it changed.
///
/// Records the finding on `Parser.source`, or clears it when the setting is
/// off, so the attribute always describes the parse that just happened.
fn read_source(slf: &Bound<'_, Parser>, buffer: &[u8]) -> PyResult<(Encoding, bool, usize)> {
    let py = slf.py();
    let (enabled, configured) = {
        let borrowed = slf.borrow();
        (
            borrowed.options.load_from_source,
            match &borrowed.options.encoding {
                Encoding::Utf8 => Encoding::Utf8,
                Encoding::Other(name) => Encoding::Other(name.clone()),
            },
        )
    };

    if !enabled {
        slf.borrow_mut().source = None;
        return Ok((configured, false, DEFAULT_LINE_LIMIT));
    }

    let found = scan_header(&header_text(py, buffer)?);
    let (encoding, unterminated, limit, info) = resolve_source(py, &found, &configured)?;
    slf.borrow_mut().source = Some(Py::new(py, info)?);
    Ok((encoding, unterminated, limit))
}

fn new_root(py: Python<'_>, capacity: usize) -> PyResult<(Py<Arena>, Py<PyAny>)> {
    let arena = Py::new(py, Arena::with_capacity(capacity))?;
    let root_id = {
        let mut data = arena.get().write();
        data.push_owned(-1, Some(""), "ROOT", "", "\n", Kind::Root)?
    };
    let root = make_handle(py, &arena, root_id)?;
    Ok((arena, root))
}

#[pymethods]
impl Parser {
    #[new]
    #[pyo3(signature = (*, config = None))]
    fn py_new(py: Python<'_>, config: Option<&Bound<'_, PyAny>>) -> PyResult<Parser> {
        let (arena, root) = new_root(py, 0)?;
        let options = match config {
            Some(config) => Options::from_config(config)?,
            None => {
                let defaults = default_options(py)?;
                Options {
                    strict: defaults.strict,
                    encoding: match &defaults.encoding {
                        Encoding::Utf8 => Encoding::Utf8,
                        Encoding::Other(name) => Encoding::Other(name.clone()),
                    },
                    collect: defaults.collect,
                    load_from_source: defaults.load_from_source,
                }
            }
        };
        Ok(Parser {
            arena,
            root,
            element_list: PyList::empty(py).unbind(),
            element_dictionary: PyDict::new(py).unbind(),
            options,
            errors: Vec::new(),
            source: None,
        })
    }

    /// Checks the parsed document against the GEDCOM release it declares
    ///
    /// Pass `version` to check against a release other than the one the
    /// document names. Reports what a line-and-tree view can decide: the
    /// character set, tags that belong to another release, cross-references,
    /// line lengths, and undoubled `@` signs. Cardinality and value grammars
    /// need the full schema and are not checked.
    ///
    /// :type version: str
    /// :rtype: list of Finding
    #[pyo3(signature = (version = None))]
    fn validate(slf: &Bound<'_, Self>, version: Option<&str>) -> PyResult<Py<PyList>> {
        let py = slf.py();
        let (arena, unterminated) = {
            let borrowed = slf.borrow();
            let unterminated = match &borrowed.source {
                Some(info) => info.get().unterminated_final_line,
                None => false,
            };
            (borrowed.arena.clone_ref(py), unterminated)
        };
        // The root is the arena's first node, pushed by `new_root`.
        let data = arena.get().read();
        crate::validate::run(py, &data, 0, version, unterminated)
    }

    /// What the last document's header said about itself
    ///
    /// `None` unless the parser was built with `load_from_source=True`.
    ///
    /// :rtype: SourceInfo or None
    #[getter]
    fn source(&self, py: Python<'_>) -> Option<Py<SourceInfo>> {
        self.source.as_ref().map(|info| info.clone_ref(py))
    }

    /// Lines the parser could not accept, when built with `on_error="collect"`
    ///
    /// :rtype: list of ParseError
    #[getter]
    fn errors(&self, py: Python<'_>) -> PyResult<Py<PyList>> {
        Ok(PyList::new(py, self.errors.iter().map(|e| e.clone_ref(py)))?.unbind())
    }

    /// Empties the element list and dictionary to cause `gedcom.parser.Parser.get_element_list()`
    /// and `gedcom.parser.Parser.get_element_dictionary()` to return updated data.
    ///
    /// The update gets deferred until each of the methods actually gets called.
    fn invalidate_cache(&mut self, py: Python<'_>) {
        self.element_list = PyList::empty(py).unbind();
        self.element_dictionary = PyDict::new(py).unbind();
    }

    /// Returns a list containing all elements from within the GEDCOM file
    ///
    /// By default elements are in the same order as they appeared in the file.
    ///
    /// This list gets generated on-the-fly, but gets cached. If the database
    /// was modified, you should call `gedcom.parser.Parser.invalidate_cache()` once to let this
    /// method return updated data.
    ///
    /// Consider using `gedcom.parser.Parser.get_root_element()` or `gedcom.parser.Parser.get_root_child_elements()` to access
    /// the hierarchical GEDCOM tree, unless you rarely modify the database.
    ///
    /// :rtype: list of Element
    fn get_element_list(slf: &Bound<'_, Self>) -> PyResult<Py<PyList>> {
        let py = slf.py();
        {
            let borrowed = slf.borrow();
            if !borrowed.element_list.bind(py).is_empty() {
                return Ok(borrowed.element_list.clone_ref(py));
            }
        }

        let list = slf.borrow().element_list.clone_ref(py);
        let bound = list.bind(py);

        // Walking the arena directly matters for more than this call: going
        // through `get_child_elements()` would materialise every node's child
        // list, and every later query would then take the slow path.
        if slf.as_any().is_exact_instance_of::<Parser>() {
            let arena = slf.borrow().arena.clone_ref(py);
            let mut ids: Vec<u32> = Vec::new();
            let complete = {
                let arena_ref = arena.get();
                let data = arena_ref.read();
                element::collect_descendants(&data, 0, &mut ids)
            };
            if complete {
                for handle in element::make_handles(py, &arena, &ids)? {
                    bound.append(handle)?;
                }
                return Ok(list);
            }
        }

        // Depth-first, iteratively: a pathologically deep file would blow the
        // stack otherwise.
        let roots = root_children_of(slf)?;
        let mut stack: Vec<Child> = Vec::new();
        for item in roots.bind(py).try_iter()?.collect::<PyResult<Vec<_>>>()?.into_iter().rev() {
            stack.push(Child::Object(item.unbind()));
        }

        while let Some(child) = stack.pop() {
            bound.append(child.handle(py)?)?;
            let grandchildren = child.children(py)?;
            for grandchild in grandchildren.into_iter().rev() {
                stack.push(grandchild);
            }
        }

        Ok(list)
    }

    /// Returns a dictionary containing all elements, identified by a pointer, from within the GEDCOM file
    ///
    /// Only elements identified by a pointer are listed in the dictionary.
    /// The keys for the dictionary are the pointers.
    ///
    /// This dictionary gets generated on-the-fly, but gets cached. If the
    /// database was modified, you should call `invalidate_cache()` once to let
    /// this method return updated data.
    ///
    /// :rtype: dict of Element
    fn get_element_dictionary(slf: &Bound<'_, Self>) -> PyResult<Py<PyDict>> {
        let py = slf.py();
        {
            let borrowed = slf.borrow();
            if !borrowed.element_dictionary.bind(py).is_empty() {
                return Ok(borrowed.element_dictionary.clone_ref(py));
            }
        }

        let dictionary = slf.borrow().element_dictionary.clone_ref(py);
        let bound = dictionary.bind(py);

        let roots = root_children_of(slf)?;
        for item in roots.bind(py).try_iter()? {
            let element = item?;
            let pointer = element.call_method0("get_pointer")?;
            if pointer.is_truthy()? {
                bound.set_item(pointer, element)?;
            }
        }

        Ok(dictionary)
    }

    /// Returns a virtual root element containing all logical records as children
    ///
    /// When printed, this element converts to an empty string.
    ///
    /// :rtype: RootElement
    fn get_root_element(&self, py: Python<'_>) -> Py<PyAny> {
        self.root.clone_ref(py)
    }

    /// Returns a list of logical records in the GEDCOM file
    ///
    /// By default, elements are in the same order as they appeared in the file.
    ///
    /// :rtype: list of Element
    fn get_root_child_elements(slf: &Bound<'_, Self>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let root = root_of(slf)?;
        let root = root.bind(py);
        if let Ok(element) = root.cast::<Element>() {
            if element::is_builtin_element(root) {
                return Ok(element.borrow().get_child_elements(py)?.into_any());
            }
        }
        Ok(root.call_method0("get_child_elements")?.unbind())
    }

    /// Opens and parses a file, from the given file path, as GEDCOM 5.5 formatted data
    /// :type file_path: str
    /// :type strict: bool
    #[pyo3(signature = (file_path, strict = None))]
    fn parse_file(
        slf: &Bound<'_, Self>,
        file_path: &Bound<'_, PyAny>,
        strict: Option<bool>,
    ) -> PyResult<()> {
        let py = slf.py();
        // Resolved before anything else: a subclass overriding `parse` declares
        // `strict=True` and would read `None` as false.
        let strict = strict.unwrap_or_else(|| slf.borrow().options.strict);

        let builtins = py.import("builtins")?;
        let stream = builtins.call_method1("open", (file_path, "rb"))?;

        // A subclass may have overridden `parse`, so only the exact class gets
        // the whole-file fast path.
        if !slf.as_any().is_exact_instance_of::<Parser>() {
            let outcome = slf.as_any().call_method1("parse", (&stream, strict));
            stream.call_method0("close")?;
            outcome?;
            return Ok(());
        }

        let read = stream.call_method0("read");
        stream.call_method0("close")?;
        let buffer: Vec<u8> = read?.extract()?;

        // The header is read before anything is decoded: it is what says how
        // to decode the rest, and which of the writer's quirks to expect.
        let (encoding, allow_unterminated, line_limit) = read_source(slf, &buffer)?;

        // Anything but UTF-8 is decoded whole and up front: its byte offsets no
        // longer line up with the text, and for UTF-16 a `\n` byte can sit
        // inside a code unit where splitting raw bytes would be wrong.
        let decoded: Option<String> = match &encoding {
            Encoding::Utf8 => None,
            Encoding::Other(name) => Some(
                PyBytes::new(py, &buffer)
                    .call_method1("decode", (name.as_str(),))?
                    .extract()?,
            ),
        };

        // The UTF-8 path keeps only the valid prefix as text but still counts
        // lines across the whole file, so a line reaching past the prefix is
        // reported as a decode failure -- and only once parsing gets that far,
        // leaving any earlier format error to win. Splitting the truncated
        // prefix instead would lose the bad line entirely.
        let (source, valid_up_to, line_ranges) = match &decoded {
            Some(text) => (
                text.as_str(),
                text.len(),
                scanner::line_ranges(text.as_bytes()),
            ),
            None => {
                let cut = match std::str::from_utf8(&buffer) {
                    Ok(_) => buffer.len(),
                    Err(error) => error.valid_up_to(),
                };
                let prefix = std::str::from_utf8(&buffer[..cut]).expect("prefix is valid");
                (prefix, cut, scanner::line_ranges(&buffer))
            }
        };
        let (arena, root) = new_root(py, line_ranges.len() + 1)?;
        arena.get().write().set_line_limit(line_limit);

        let base = arena.get().write().adopt_text(source)? as usize;
        let lines: Vec<(usize, usize)> = line_ranges
            .iter()
            .map(|(start, len)| {
                let (start, len) = strip_bom_range(source, *start, *len);
                (base + start, len)
            })
            .collect();

        {
            let mut borrowed = slf.borrow_mut();
            borrowed.invalidate_cache(py);
            borrowed.arena = arena.clone_ref(py);
            borrowed.root = root;
        }

        let collect = slf.borrow().options.collect;
        let mut faults: Vec<(usize, ParseFault)> = Vec::new();
        let outcome = {
            let mut data = arena.get().write();
            parse_buffer(
                &mut data,
                &lines,
                base + valid_up_to,
                0,
                strict,
                collect,
                allow_unterminated,
                &mut faults,
            )
        };

        if let Err(fault) = outcome {
            return Err(fault.into_error(py, Some(&buffer), &line_ranges));
        }
        record_faults(slf, py, faults, |index| {
            let (start, len) = line_ranges[index];
            String::from_utf8_lossy(&buffer[start..start + len]).into_owned()
        })
    }

    /// Parses a stream, or an array of lines, as GEDCOM 5.5 formatted data
    /// :type gedcom_stream: a file stream, or bytes array of lines with new line at the end
    /// :type strict: bool
    #[pyo3(signature = (gedcom_stream, strict = None))]
    fn parse(
        slf: &Bound<'_, Self>,
        gedcom_stream: &Bound<'_, PyAny>,
        strict: Option<bool>,
    ) -> PyResult<()> {
        let py = slf.py();
        let strict = strict.unwrap_or_else(|| slf.borrow().options.strict);
        let (arena, root) = new_root(py, 0)?;
        {
            let mut borrowed = slf.borrow_mut();
            borrowed.invalidate_cache(py);
            borrowed.arena = arena.clone_ref(py);
            borrowed.root = root;
        }

        // Reading the header means holding back the lines it occupies, so the
        // stream is only drained that far before anything is decoded.
        let mut items = gedcom_stream.try_iter()?;
        let (encoding, allow_unterminated, line_limit, held) =
            read_source_from_stream(slf, &mut items)?;
        arena.get().write().set_line_limit(line_limit);

        let (native_utf8, codec) = match &encoding {
            Encoding::Utf8 => (true, "utf-8-sig".to_owned()),
            Encoding::Other(name) => (false, name.clone()),
        };
        let codec = codec.as_str();

        let collect = slf.borrow().options.collect;
        let mut faults: Vec<(usize, ParseFault, String)> = Vec::new();
        let mut last_id = 0u32;
        let mut line_number = 1usize;

        // The lock is taken per line: pulling the next item runs Python code,
        // which must never happen with a guard held.
        for item in held.into_iter().map(Ok).chain(items) {
            let item = item?;
            let text: String = match item.cast::<PyBytes>() {
                // The zero-copy read only applies to UTF-8; any other codec
                // has to go through CPython.
                Ok(bytes) if native_utf8 => match std::str::from_utf8(bytes.as_bytes()) {
                    Ok(text) => scanner::strip_bom(text).to_owned(),
                    Err(_) => return Err(decode_error(py, bytes.as_bytes(), codec)),
                },
                Ok(bytes) => bytes.as_any().call_method1("decode", (codec,))?.extract()?,
                Err(_) => item.call_method1("decode", (codec,))?.extract()?,
            };

            let outcome = {
                let mut data = arena.get().write();
                // Appending to the shared buffer rather than allocating a
                // string per line; the spans then point into it as usual.
                match data.adopt_text(&text) {
                    Ok(start) => parse_one_line(
                        &mut data,
                        start as usize,
                        text.len(),
                        line_number,
                        last_id,
                        strict,
                        allow_unterminated,
                    ),
                    Err(error) => return Err(error),
                }
            };

            match outcome {
                Ok(id) => last_id = id,
                Err(fault) if collect => faults.push((line_number - 1, fault, text.clone())),
                Err(fault) => return Err(fault.into_error(py, None, &[])),
            }
            line_number += 1;
        }

        let recorded: Vec<Py<ParseError>> = faults
            .into_iter()
            .map(|(index, fault, line)| build_error(py, index, &line, &fault))
            .collect::<PyResult<_>>()?;
        slf.borrow_mut().errors = recorded;
        Ok(())
    }

    /// Returns a list of marriages of an individual formatted as a tuple (`str` date, `str` place)
    /// :type individual: IndividualElement
    /// :rtype: list of tuple
    fn get_marriages(slf: &Bound<'_, Self>, individual: &Bound<'_, PyAny>) -> PyResult<Py<PyList>> {
        let py = slf.py();
        require_individual(individual)?;

        let mut marriages: Vec<Py<PyAny>> = Vec::new();
        let families = families_of(slf, individual, tags::FAMILY_SPOUSE)?;

        for family in families.bind(py).try_iter()? {
            let family = family?;
            for family_data in children_of(py, &family)? {
                if !family_data.tag_matches(py, tags::MARRIAGE)? {
                    continue;
                }
                let mut date = String::new();
                let mut place = String::new();
                for marriage_data in family_data.children(py)? {
                    let tag = marriage_data.tag(py)?;
                    if tag == tags::DATE {
                        date = marriage_data.value_str(py)?;
                    }
                    if tag == tags::PLACE {
                        place = marriage_data.value_str(py)?;
                    }
                }
                marriages.push(PyTuple::new(py, [date, place])?.into_any().unbind());
            }
        }

        Ok(PyList::new(py, marriages)?.unbind())
    }

    /// Returns a list of marriage years (as integers) for an individual
    /// :type individual: IndividualElement
    /// :rtype: list of int
    fn get_marriage_years(
        slf: &Bound<'_, Self>,
        individual: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyList>> {
        let py = slf.py();
        require_individual(individual)?;

        let mut dates: Vec<Py<PyAny>> = Vec::new();
        let families = families_of(slf, individual, tags::FAMILY_SPOUSE)?;

        for family in families.bind(py).try_iter()? {
            let family = family?;
            for child in children_of(py, &family)? {
                if !child.tag_matches(py, tags::MARRIAGE)? {
                    continue;
                }
                for grandchild in child.children(py)? {
                    if grandchild.tag(py)? != tags::DATE {
                        continue;
                    }
                    let value = grandchild.value_str(py)?;
                    let token = pystr::last_whitespace_token(&value).ok_or_else(|| {
                        pyo3::exceptions::PyIndexError::new_err("list index out of range")
                    })?;
                    if let Some(year) = pystr::python_int(py, token)? {
                        dates.push(year);
                    }
                }
            }
        }

        Ok(PyList::new(py, dates)?.unbind())
    }

    /// Checks if one of the marriage years of an individual matches the supplied year. Year is an integer.
    /// :type individual: IndividualElement
    /// :type year: int
    /// :rtype: bool
    fn marriage_year_match(
        slf: &Bound<'_, Self>,
        individual: &Bound<'_, PyAny>,
        year: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        require_individual(individual)?;
        let years = marriage_years_of(slf, individual)?;
        years.bind(slf.py()).contains(year)
    }

    /// Check if one of the marriage years of an individual is in a given range. Years are integers.
    /// :type individual: IndividualElement
    /// :type from_year: int
    /// :type to_year: int
    /// :rtype: bool
    fn marriage_range_match(
        slf: &Bound<'_, Self>,
        individual: &Bound<'_, PyAny>,
        from_year: &Bound<'_, PyAny>,
        to_year: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        require_individual(individual)?;
        let years = marriage_years_of(slf, individual)?;
        for year in years.bind(slf.py()).try_iter()? {
            let year = year?;
            if from_year.le(&year)? && year.le(to_year)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Return family elements listed for an individual
    ///
    /// family_type can be `gedcom.tags.GEDCOM_TAG_FAMILY_SPOUSE` (families where the individual is a spouse) or
    /// `gedcom.tags.GEDCOM_TAG_FAMILY_CHILD` (families where the individual is a child). If a value is not
    /// provided, `gedcom.tags.GEDCOM_TAG_FAMILY_SPOUSE` is default value.
    ///
    /// :type individual: IndividualElement
    /// :type family_type: str
    /// :rtype: list of FamilyElement
    #[pyo3(signature = (individual, family_type = tags::FAMILY_SPOUSE))]
    fn get_families(
        slf: &Bound<'_, Self>,
        individual: &Bound<'_, PyAny>,
        family_type: &str,
    ) -> PyResult<Py<PyList>> {
        let py = slf.py();
        require_individual(individual)?;

        let dictionary = dictionary_of(slf)?;
        let dictionary = dictionary.bind(py);
        let pointers = tagged_child_values(py, individual, family_type)?;

        let mut families: Vec<Py<PyAny>> = Vec::new();
        for pointer in pointers {
            if let Some(family) = lookup(dictionary, pointer.bind(py))? {
                families.push(family);
            }
        }

        Ok(PyList::new(py, families)?.unbind())
    }

    /// Return elements corresponding to ancestors of an individual
    ///
    /// Optional `ancestor_type`. Default "ALL" returns all ancestors, "NAT" can be
    /// used to specify only natural (genetic) ancestors.
    ///
    /// :type individual: IndividualElement
    /// :type ancestor_type: str
    /// :rtype: list of IndividualElement
    #[pyo3(signature = (individual, ancestor_type = "ALL"))]
    fn get_ancestors(
        slf: &Bound<'_, Self>,
        individual: &Bound<'_, PyAny>,
        ancestor_type: &str,
    ) -> PyResult<Py<PyList>> {
        let py = slf.py();
        require_individual(individual)?;

        let _guard = RecursionGuard::enter(py)?;

        let parents = parents_of(slf, individual, ancestor_type)?;
        let parents = parents.bind(py);

        let ancestors = PyList::empty(py);
        for parent in parents.try_iter()? {
            ancestors.append(parent?)?;
        }

        // The recursive call drops back to the default type, as the original
        // does -- `ancestor_type` is deliberately not passed down.
        for parent in parents.try_iter()? {
            let deeper = ancestors_of(slf, &parent?, tags::MEMBERS_ALL)?;
            for ancestor in deeper.bind(py).try_iter()? {
                ancestors.append(ancestor?)?;
            }
        }

        Ok(ancestors.unbind())
    }

    /// Return elements corresponding to parents of an individual
    ///
    /// Optional parent_type. Default "ALL" returns all parents. "NAT" can be
    /// used to specify only natural (genetic) parents.
    ///
    /// :type individual: IndividualElement
    /// :type parent_type: str
    /// :rtype: list of IndividualElement
    #[pyo3(signature = (individual, parent_type = "ALL"))]
    fn get_parents(
        slf: &Bound<'_, Self>,
        individual: &Bound<'_, PyAny>,
        parent_type: &str,
    ) -> PyResult<Py<PyList>> {
        let py = slf.py();
        require_individual(individual)?;

        let parents = PyList::empty(py);
        let families = families_of(slf, individual, tags::FAMILY_CHILD)?;

        let pointer = pointer_of(individual)?;
        let pointer = pointer.bind(py);

        for family in families.bind(py).try_iter()? {
            let family = family?;

            if parent_type != "NAT" {
                extend_list(&parents, &family_members_of(slf, &family, tags::MEMBERS_PARENTS)?)?;
                continue;
            }

            for family_member in children_of(py, &family)? {
                if !family_member.tag_matches(py, tags::CHILD)? {
                    continue;
                }
                if !family_member.value(py)?.bind(py).eq(pointer)? {
                    continue;
                }
                for child in family_member.children(py)? {
                    if child.value_str(py)? != "Natural" {
                        continue;
                    }
                    let tag = child.tag(py)?;
                    let wanted = if tag == tags::MREL {
                        tags::WIFE
                    } else if tag == tags::FREL {
                        tags::HUSBAND
                    } else {
                        continue;
                    };
                    extend_list(&parents, &family_members_of(slf, &family, wanted)?)?;
                }
            }
        }

        Ok(parents.unbind())
    }

    /// Return path from descendant to ancestor
    /// :rtype: list of IndividualElement or None
    #[pyo3(signature = (descendant, ancestor, path = None))]
    fn find_path_to_ancestor(
        slf: &Bound<'_, Self>,
        descendant: &Bound<'_, PyAny>,
        ancestor: &Bound<'_, PyAny>,
        path: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let _guard = RecursionGuard::enter(py)?;

        // The original's condition is `not isinstance(descendant, ...) and
        // isinstance(ancestor, ...)`, which only fires when the ancestor is an
        // individual and the descendant is not.
        if !descendant.is_instance_of::<IndividualElement>()
            && ancestor.is_instance_of::<IndividualElement>()
        {
            return Err(crate::individual::NotAnActualIndividualError::new_err(
                format!(
                    "Operation only valid for elements with {} tag.",
                    tags::INDIVIDUAL
                ),
            ));
        }

        let path = match path {
            Some(existing) if existing.is_truthy()? => existing.clone(),
            _ => PyList::new(py, [descendant])?.into_any(),
        };

        let last = path.get_item(path.len()? - 1)?;
        if last
            .call_method0("get_pointer")?
            .eq(ancestor.call_method0("get_pointer")?)?
        {
            return Ok(path.unbind());
        }

        let parents = parents_of(slf, descendant, "NAT")?;
        for parent in parents.bind(py).try_iter()? {
            let parent = parent?;
            let extended = path.call_method1("__add__", (PyList::new(py, [&parent])?,))?;
            let potential = path_to_ancestor_of(slf, &parent, ancestor, &extended)?;
            if !potential.is_none(py) {
                return Ok(potential);
            }
        }

        Ok(py.None())
    }

    /// Return array of family members: individual, spouse, and children
    ///
    /// Optional argument `members_type` can be used to return specific subsets:
    ///
    /// "FAMILY_MEMBERS_TYPE_ALL": Default, return all members of the family
    /// "FAMILY_MEMBERS_TYPE_PARENTS": Return individuals with "HUSB" and "WIFE" tags (parents)
    /// "FAMILY_MEMBERS_TYPE_HUSBAND": Return individuals with "HUSB" tags (father)
    /// "FAMILY_MEMBERS_TYPE_WIFE": Return individuals with "WIFE" tags (mother)
    /// "FAMILY_MEMBERS_TYPE_CHILDREN": Return individuals with "CHIL" tags (children)
    ///
    /// :type family: FamilyElement
    /// :type members_type: str
    /// :rtype: list of IndividualElement
    #[pyo3(signature = (family, members_type = tags::MEMBERS_ALL))]
    fn get_family_members(
        slf: &Bound<'_, Self>,
        family: &Bound<'_, PyAny>,
        members_type: &str,
    ) -> PyResult<Py<PyList>> {
        let py = slf.py();
        if !family.is_instance_of::<crate::element::FamilyElement>() {
            return Err(NotAnActualFamilyError::new_err(format!(
                "Operation only valid for element with {} tag.",
                tags::FAMILY
            )));
        }

        let dictionary = dictionary_of(slf)?;
        let dictionary = dictionary.bind(py);

        let selector = |tag: &str| match members_type {
            tags::MEMBERS_PARENTS => tag == tags::HUSBAND || tag == tags::WIFE,
            tags::HUSBAND => tag == tags::HUSBAND,
            tags::WIFE => tag == tags::WIFE,
            tags::CHILD => tag == tags::CHILD,
            _ => tag == tags::HUSBAND || tag == tags::WIFE || tag == tags::CHILD,
        };

        let pointers = with_children(
            py,
            family,
            |data, id| {
                let Some(ids) = data.children_of(id) else {
                    return Ok(None);
                };
                let mut values = Vec::new();
                for child_id in ids {
                    if selector(data.effective_tag_of(child_id)) {
                        values.push(
                            data.value_of(child_id)
                                .into_pyobject(py)?
                                .into_any()
                                .unbind(),
                        );
                    }
                }
                Ok(Some(values))
            },
            |children| {
                let mut values = Vec::new();
                for child in children {
                    if selector(&child.tag(py)?) {
                        values.push(child.value(py)?);
                    }
                }
                Ok(values)
            },
        )?;

        let mut members: Vec<Py<PyAny>> = Vec::new();
        for pointer in pointers {
            if let Some(member) = lookup(dictionary, pointer.bind(py))? {
                members.push(member);
            }
        }

        Ok(PyList::new(py, members)?.unbind())
    }

    /// Write GEDCOM data to stdout
    fn print_gedcom(slf: &Bound<'_, Self>) -> PyResult<()> {
        let stdout = slf.py().import("sys")?.getattr("stdout")?;
        slf.as_any().call_method1("save_gedcom", (stdout,))?;
        Ok(())
    }

    /// Save GEDCOM data to a file
    /// :type open_file: file
    fn save_gedcom(slf: &Bound<'_, Self>, open_file: &Bound<'_, PyAny>) -> PyResult<()> {
        let root = root_of(slf)?;
        let rendered = root.bind(slf.py()).call_method1("to_gedcom_string", (true,))?;
        open_file.call_method1("write", (rendered,))?;
        Ok(())
    }

    fn __traverse__(&self, visit: pyo3::PyVisit<'_>) -> Result<(), pyo3::PyTraverseError> {
        visit.call(&self.arena)?;
        visit.call(&self.root)?;
        visit.call(&self.element_list)?;
        visit.call(&self.element_dictionary)?;
        Ok(())
    }
}

//
// The original calls its own methods through `self`, so a subclass override has
// to win. When `type(slf)` is exactly `Parser` no override can exist, and the
// call goes straight to the Rust function instead -- each interpreter round trip
// costs 100-250ns, and these sit inside loops.
//
// Every helper below MUST keep both branches: dropping the `call_method` arm
// breaks subclassing silently. `tests/test_subclassing.py` guards each one.

thread_local! {
    static RECURSION_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static RECURSION_LIMIT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

struct RecursionGuard;

impl RecursionGuard {
    fn enter(py: Python<'_>) -> PyResult<Self> {
        let depth = RECURSION_DEPTH.with(|depth| {
            let next = depth.get() + 1;
            depth.set(next);
            next
        });

        if depth == 1 {
            let limit: usize = py
                .import("sys")?
                .call_method0("getrecursionlimit")?
                .extract()?;
            RECURSION_LIMIT.with(|cell| cell.set(limit));
        }

        if depth > RECURSION_LIMIT.with(|cell| cell.get()) {
            RECURSION_DEPTH.with(|cell| cell.set(cell.get().saturating_sub(1)));
            return Err(pyo3::exceptions::PyRecursionError::new_err(
                "maximum recursion depth exceeded",
            ));
        }

        Ok(RecursionGuard)
    }
}

impl Drop for RecursionGuard {
    fn drop(&mut self) {
        RECURSION_DEPTH.with(|cell| cell.set(cell.get().saturating_sub(1)));
    }
}

fn ancestors_of(
    slf: &Bound<'_, Parser>,
    individual: &Bound<'_, PyAny>,
    ancestor_type: &str,
) -> PyResult<Py<PyAny>> {
    if is_plain(slf) {
        return Ok(Parser::get_ancestors(slf, individual, ancestor_type)?.into_any());
    }
    Ok(slf
        .as_any()
        .call_method1("get_ancestors", (individual, ancestor_type))?
        .unbind())
}

fn path_to_ancestor_of(
    slf: &Bound<'_, Parser>,
    descendant: &Bound<'_, PyAny>,
    ancestor: &Bound<'_, PyAny>,
    path: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    if is_plain(slf) {
        return Parser::find_path_to_ancestor(slf, descendant, ancestor, Some(path));
    }
    Ok(slf
        .as_any()
        .call_method1("find_path_to_ancestor", (descendant, ancestor, path))?
        .unbind())
}

fn is_plain(slf: &Bound<'_, Parser>) -> bool {
    slf.as_any().is_exact_instance_of::<Parser>()
}

fn dictionary_of(slf: &Bound<'_, Parser>) -> PyResult<Py<PyAny>> {
    if is_plain(slf) {
        return Ok(Parser::get_element_dictionary(slf)?.into_any());
    }
    Ok(slf.as_any().call_method0("get_element_dictionary")?.unbind())
}

fn root_children_of(slf: &Bound<'_, Parser>) -> PyResult<Py<PyAny>> {
    if is_plain(slf) {
        return Parser::get_root_child_elements(slf);
    }
    Ok(slf.as_any().call_method0("get_root_child_elements")?.unbind())
}

fn families_of(
    slf: &Bound<'_, Parser>,
    individual: &Bound<'_, PyAny>,
    family_type: &str,
) -> PyResult<Py<PyAny>> {
    if is_plain(slf) {
        return Ok(Parser::get_families(slf, individual, family_type)?.into_any());
    }
    Ok(slf
        .as_any()
        .call_method1("get_families", (individual, family_type))?
        .unbind())
}

fn family_members_of(
    slf: &Bound<'_, Parser>,
    family: &Bound<'_, PyAny>,
    members_type: &str,
) -> PyResult<Py<PyAny>> {
    if is_plain(slf) {
        return Ok(Parser::get_family_members(slf, family, members_type)?.into_any());
    }
    Ok(slf
        .as_any()
        .call_method1("get_family_members", (family, members_type))?
        .unbind())
}

fn marriage_years_of(
    slf: &Bound<'_, Parser>,
    individual: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    if is_plain(slf) {
        return Ok(Parser::get_marriage_years(slf, individual)?.into_any());
    }
    Ok(slf
        .as_any()
        .call_method1("get_marriage_years", (individual,))?
        .unbind())
}

fn parents_of(
    slf: &Bound<'_, Parser>,
    individual: &Bound<'_, PyAny>,
    parent_type: &str,
) -> PyResult<Py<PyAny>> {
    if is_plain(slf) {
        return Ok(Parser::get_parents(slf, individual, parent_type)?.into_any());
    }
    Ok(slf
        .as_any()
        .call_method1("get_parents", (individual, parent_type))?
        .unbind())
}

fn root_of(slf: &Bound<'_, Parser>) -> PyResult<Py<PyAny>> {
    if is_plain(slf) {
        return Ok(slf.borrow().get_root_element(slf.py()));
    }
    Ok(slf.as_any().call_method0("get_root_element")?.unbind())
}

fn pointer_of(element: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let py = element.py();
    if element::is_builtin_element(element) {
        if let Ok(inner) = element.cast::<Element>() {
            return Ok(match inner.borrow().get_pointer(py) {
                Some(text) => text.into_pyobject(py)?.into_any().unbind(),
                None => py.None(),
            });
        }
    }
    Ok(element.call_method0("get_pointer")?.unbind())
}

fn extend_list(target: &Bound<'_, PyList>, items: &Py<PyAny>) -> PyResult<()> {
    for item in items.bind(target.py()).try_iter()? {
        target.append(item?)?;
    }
    Ok(())
}

fn lookup(
    dictionary: &Bound<'_, PyAny>,
    key: &Bound<'_, PyAny>,
) -> PyResult<Option<Py<PyAny>>> {
    if let Ok(mapping) = dictionary.cast::<PyDict>() {
        return Ok(mapping.get_item(key)?.map(|value| value.unbind()));
    }
    if dictionary.contains(key)? {
        return Ok(Some(dictionary.get_item(key)?.unbind()));
    }
    Ok(None)
}

fn tagged_child_values(
    py: Python<'_>,
    object: &Bound<'_, PyAny>,
    tag: &str,
) -> PyResult<Vec<Py<PyAny>>> {
    with_children(
        py,
        object,
        |data, id| {
            let Some(ids) = data.children_of(id) else {
                return Ok(None);
            };
            let mut values = Vec::new();
            for child_id in ids {
                if data.effective_tag_of(child_id) == tag {
                    values.push(
                        data.value_of(child_id)
                            .into_pyobject(py)?
                            .into_any()
                            .unbind(),
                    );
                }
            }
            Ok(Some(values))
        },
        |children| {
            let mut values = Vec::new();
            for child in children {
                if child.tag_matches(py, tag)? {
                    values.push(child.value(py)?);
                }
            }
            Ok(values)
        },
    )
}
