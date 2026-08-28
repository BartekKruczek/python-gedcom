"""Type stub for the compiled GEDCOM core."""

from typing import IO, Any, Iterable, Protocol

class _ConfigLike(Protocol):
    """Anything shaped like `gedcom.config.ParserConfig`.

    The parser reads these three attributes by name, so a foreign model or a
    plain object works just as well as the pydantic one.
    """

    @property
    def strict(self) -> bool: ...
    @property
    def encoding(self) -> str: ...
    @property
    def on_error(self) -> str: ...

class GedcomFormatViolationError(Exception): ...
class NotAnActualFamilyError(Exception): ...
class NotAnActualFileError(Exception): ...
class NotAnActualIndividualError(Exception): ...
class NotAnActualObjectError(Exception): ...

class Arena: ...

class ParseError:
    """One line the parser could not accept, kept when `on_error` is `"collect"`."""

    @property
    def error_type(self) -> str:
        """The name of the exception class that would have been raised

        :rtype: str
        """
    @property
    def line(self) -> str:
        """The offending line, terminator included

        :rtype: str
        """
    @property
    def line_number(self) -> int:
        """The 1-based number of the offending line

        :rtype: int
        """
    @property
    def message(self) -> str:
        """The message the parser would have raised

        :rtype: str
        """

class SourceInfo:
    """What the document's own header said about itself.

    Present on `Parser.source` after a parse with `load_from_source` set, and
    `None` otherwise.
    """

    @property
    def character_set(self) -> str | None:
        """The `HEAD.CHAR` value, verbatim

        :rtype: str or None
        """
    @property
    def encoding(self) -> str:
        """The codec the document was actually read with

        :rtype: str
        """
    @property
    def gedcom_version(self) -> str | None:
        """The `HEAD.GEDC.VERS` value -- which GEDCOM release the file claims

        :rtype: str or None
        """
    @property
    def name(self) -> str | None:
        """The program's name, if it is one `gedcom.config.sources` knows

        :rtype: str or None
        """
    @property
    def notes(self) -> list[str]:
        """What the header said that does not add up, in plain words

        Never fatal: a mislabelled header is ordinary, and the file still reads.

        :rtype: list of str
        """
    @property
    def system(self) -> str | None:
        """The `HEAD.SOUR` value, verbatim

        :rtype: str or None
        """
    @property
    def unterminated_final_line(self) -> bool:
        """Whether the final line was allowed to carry no terminator

        :rtype: bool
        """
    @property
    def version(self) -> str | None:
        """The `HEAD.SOUR.VERS` value -- the version of the program, not of GEDCOM

        :rtype: str or None
        """

class Finding:
    """One thing wrong with a document."""

    @property
    def line_number(self) -> int:
        """The 1-based line the finding is about, or `0` for the document as a whole

        :rtype: int
        """
    @property
    def message(self) -> str:
        """What is wrong, in plain words

        :rtype: str
        """
    @property
    def rule(self) -> str:
        """A stable identifier for the rule, such as `"tag-not-in-version"`

        :rtype: str
        """
    @property
    def severity(self) -> str:
        """`"error"` for a violation, `"warning"` for something merely suspect

        :rtype: str
        """
    @property
    def tag(self) -> str | None:
        """The tag of the offending line, if the finding is about one

        :rtype: str or None
        """

class Element:
    """GEDCOM element

    Each line in a GEDCOM file is an element with the format

    `level [pointer] tag [value]`

    where `level` and `tag` are required, and `pointer` and `value` are
    optional.  Elements are arranged hierarchically according to their
    level, and elements with a level of zero are at the top level.
    Elements with a level greater than zero are children of their
    parent.

    A pointer has the format `@pname@`, where `pname` is any sequence of
    characters and numbers. The pointer identifies the object being
    pointed to, so that any pointer included as the value of any
    element points back to the original object.  For example, an
    element may have a `FAMS` tag whose value is `@F1@`, meaning that this
    element points to the family record in which the associated person
    is a spouse. Likewise, an element with a tag of `FAMC` has a value
    that points to a family record in which the associated person is a
    child.

    See a GEDCOM file for examples of tags and their values.

    Tags available to an element are seen here: `gedcom.tags`
    """

    def __init__(self, level: int, pointer: str | None, tag: str, value: str, crlf: str = "\n", multi_line: bool = True) -> None: ...
    def add_child_element(self, element: Element) -> Element:
        """Adds a child element to this element

        :type element: Element
        """
    def get_child_elements(self) -> list[Element]:
        """Returns the direct child elements of this element
        :rtype: list of Element
        """
    def get_level(self) -> int:
        """Returns the level of this element from within the GEDCOM file
        :rtype: int
        """
    def get_multi_line_value(self) -> str:
        """Returns the value of this element including concatenations or continuations
        :rtype: str
        """
    def get_parent_element(self) -> Element | None:
        """Returns the parent element of this element
        :rtype: Element or None
        """
    def get_pointer(self) -> str | None:
        """Returns the pointer of this element from within the GEDCOM file
        :rtype: str or None
        """
    def get_tag(self) -> str:
        """Returns the tag of this element from within the GEDCOM file
        :rtype: str
        """
    def get_value(self) -> str:
        """Return the value of this element from within the GEDCOM file
        :rtype: str
        """
    def new_child_element(self, tag: str, pointer: str = "", value: str = "") -> Element:
        """Creates and returns a new child element of this element

        :type tag: str
        :type pointer: str
        :type value: str
        :rtype: Element
        """
    def set_multi_line_value(self, value: str) -> None:
        """Sets the value of this element, adding concatenation and continuation lines when necessary
        :type value: str
        """
    def set_parent_element(self, element: Element) -> None:
        """Adds a parent element to this element

        There's usually no need to call this method manually,
        `add_child_element()` calls it automatically.

        :type element: Element
        """
    def set_value(self, value: str) -> None:
        """Sets the value of this element
        :type value: str
        """
    def to_gedcom_string(self, recursive: bool = False) -> str:
        """Formats this element and optionally all of its sub-elements into a GEDCOM string
        :type recursive: bool
        :rtype: str
        """

class FamilyElement(Element):
    def __init__(self, level: int, pointer: str | None, tag: str, value: str, crlf: str = "\n", multi_line: bool = True) -> None: ...
    def get_tag(self) -> str:
        """Returns the tag of this element from within the GEDCOM file
        :rtype: str
        """

class FileElement(Element):
    def __init__(self, level: int, pointer: str | None, tag: str, value: str, crlf: str = "\n", multi_line: bool = True) -> None: ...
    def get_tag(self) -> str:
        """Returns the tag of this element from within the GEDCOM file
        :rtype: str
        """

class IndividualElement(Element):
    """GEDCOM element consisting of tag `INDI`."""

    def __init__(self, level: int, pointer: str | None, tag: str, value: str, crlf: str = "\n", multi_line: bool = True) -> None: ...
    def birth_range_match(self, from_year: int, to_year: int) -> bool:
        """Checks if the birth year of a person lies within the given range
        :type from_year: int
        :type to_year: int
        :rtype: bool
        """
    def birth_year_match(self, year: int) -> bool:
        """Returns `True` if the given year matches the birth year of this person
        :type year: int
        :rtype: bool
        """
    def criteria_match(self, criteria: str) -> bool:
        """Checks if this individual matches all of the given criteria

        `criteria` is a colon-separated list, where each item in the
        list has the form [name]=[value]. The following criteria are supported:

        surname=[name]
             Match a person with [name] in any part of the `surname`.
        given_name=[given_name]
             Match a person with [given_name] in any part of the given `given_name`.
        birth=[year]
             Match a person whose birth year is a four-digit [year].
        birth_range=[from_year-to_year]
             Match a person whose birth year is in the range of years from
             [from_year] to [to_year], including both [from_year] and [to_year].

        :type criteria: str
        :rtype: bool
        """
    def death_range_match(self, from_year: int, to_year: int) -> bool:
        """Checks if the death year of a person lies within the given range
        :type from_year: int
        :type to_year: int
        :rtype: bool
        """
    def death_year_match(self, year: int) -> bool:
        """Returns `True` if the given year matches the death year of this person
        :type year: int
        :rtype: bool
        """
    def get_all_names(self) -> list[str]: ...
    def get_birth_data(self) -> tuple[Any, ...]:
        """Returns the birth data of a person formatted as a tuple: (`str` date, `str` place, `list` sources)
        :rtype: tuple
        """
    def get_birth_year(self) -> int:
        """Returns the birth year of a person in integer format
        :rtype: int
        """
    def get_burial_data(self) -> tuple[Any, ...]:
        """Returns the burial data of a person formatted as a tuple: (`str` date, `str´ place, `list` sources)
        :rtype: tuple
        """
    def get_census_data(self) -> list[tuple[Any, ...]]:
        """Returns a list of censuses of an individual formatted as tuples: (`str` date, `str´ place, `list` sources)
        :rtype: list of tuple
        """
    def get_death_data(self) -> tuple[Any, ...]:
        """Returns the death data of a person formatted as a tuple: (`str` date, `str` place, `list` sources)
        :rtype: tuple
        """
    def get_death_year(self) -> int:
        """Returns the death year of a person in integer format
        :rtype: int
        """
    def get_gender(self) -> str:
        """Returns the gender of a person in string format
        :rtype: str
        """
    def get_last_change_date(self) -> str:
        """Returns the date of when the person data was last changed formatted as a string
        :rtype: str
        """
    def get_name(self) -> tuple[Any, ...]:
        """Returns an individual's names as a tuple: (`str` given_name, `str` surname)
        :rtype: tuple
        """
    def get_occupation(self) -> str:
        """Returns the occupation of a person
        :rtype: str
        """
    def get_tag(self) -> str:
        """Returns the tag of this element from within the GEDCOM file
        :rtype: str
        """
    def given_name_match(self, given_name_to_match: str) -> bool:
        """Matches a string with the given name of an individual
        :type given_name_to_match: str
        :rtype: bool
        """
    def is_child(self) -> bool:
        """Checks if this element is a child of a family
        :rtype: bool
        """
    def is_deceased(self) -> bool:
        """Checks if this individual is deceased
        :rtype: bool
        """
    def is_private(self) -> bool:
        """Checks if this individual is marked private
        :rtype: bool
        """
    def surname_match(self, surname_to_match: str) -> bool:
        """Matches a string with the surname of an individual
        :type surname_to_match: str
        :rtype: bool
        """

class ObjectElement(Element):
    """GEDCOM element consisting of tag `OBJE`."""

    def __init__(self, level: int, pointer: str | None, tag: str, value: str, crlf: str = "\n", multi_line: bool = True) -> None: ...
    def is_object(self) -> bool:
        """Checks if this element is an actual object
        :rtype: bool
        """

class RootElement(Element):
    """Virtual GEDCOM root element containing all logical records as children"""

    def __init__(self, level: int = ..., pointer: str | None = ..., tag: str = "ROOT", value: str = "", crlf: str = "\n", multi_line: bool = True) -> None: ...

class Parser:
    """Parses and manipulates GEDCOM 5.5 format data

    For documentation of the GEDCOM 5.5 format, see: http://homepages.rootsweb.ancestry.com/~pmcbride/gedcom/55gctoc.htm

    This parser reads and parses a GEDCOM file.

    Elements may be accessed via:

    * a `list` through `gedcom.parser.Parser.get_element_list()`
    * a `dict` through `gedcom.parser.Parser.get_element_dictionary()`
    """

    def __init__(self, *, config: _ConfigLike | None = None) -> None: ...
    @property
    def errors(self) -> list[ParseError]:
        """Lines the parser could not accept, when built with `on_error="collect"`

        :rtype: list of ParseError
        """
    def find_path_to_ancestor(self, descendant: IndividualElement, ancestor: IndividualElement, path: list[Element] | None = None) -> list[IndividualElement] | None:
        """Return path from descendant to ancestor
        :rtype: list of IndividualElement or None
        """
    def get_ancestors(self, individual: IndividualElement, ancestor_type: str = "ALL") -> list[IndividualElement]:
        """Return elements corresponding to ancestors of an individual

        Optional `ancestor_type`. Default "ALL" returns all ancestors, "NAT" can be
        used to specify only natural (genetic) ancestors.

        :type individual: IndividualElement
        :type ancestor_type: str
        :rtype: list of IndividualElement
        """
    def get_element_dictionary(self) -> dict[str, Element]:
        """Returns a dictionary containing all elements, identified by a pointer, from within the GEDCOM file

        Only elements identified by a pointer are listed in the dictionary.
        The keys for the dictionary are the pointers.

        This dictionary gets generated on-the-fly, but gets cached. If the
        database was modified, you should call `invalidate_cache()` once to let
        this method return updated data.

        :rtype: dict of Element
        """
    def get_element_list(self) -> list[Element]:
        """Returns a list containing all elements from within the GEDCOM file

        By default elements are in the same order as they appeared in the file.

        This list gets generated on-the-fly, but gets cached. If the database
        was modified, you should call `gedcom.parser.Parser.invalidate_cache()` once to let this
        method return updated data.

        Consider using `gedcom.parser.Parser.get_root_element()` or `gedcom.parser.Parser.get_root_child_elements()` to access
        the hierarchical GEDCOM tree, unless you rarely modify the database.

        :rtype: list of Element
        """
    def get_families(self, individual: IndividualElement, family_type: str = ...) -> list[FamilyElement]:
        """Return family elements listed for an individual

        family_type can be `gedcom.tags.GEDCOM_TAG_FAMILY_SPOUSE` (families where the individual is a spouse) or
        `gedcom.tags.GEDCOM_TAG_FAMILY_CHILD` (families where the individual is a child). If a value is not
        provided, `gedcom.tags.GEDCOM_TAG_FAMILY_SPOUSE` is default value.

        :type individual: IndividualElement
        :type family_type: str
        :rtype: list of FamilyElement
        """
    def get_family_members(self, family: FamilyElement, members_type: str = ...) -> list[IndividualElement]:
        """Return array of family members: individual, spouse, and children

        Optional argument `members_type` can be used to return specific subsets:

        "FAMILY_MEMBERS_TYPE_ALL": Default, return all members of the family
        "FAMILY_MEMBERS_TYPE_PARENTS": Return individuals with "HUSB" and "WIFE" tags (parents)
        "FAMILY_MEMBERS_TYPE_HUSBAND": Return individuals with "HUSB" tags (father)
        "FAMILY_MEMBERS_TYPE_WIFE": Return individuals with "WIFE" tags (mother)
        "FAMILY_MEMBERS_TYPE_CHILDREN": Return individuals with "CHIL" tags (children)

        :type family: FamilyElement
        :type members_type: str
        :rtype: list of IndividualElement
        """
    def get_marriage_years(self, individual: IndividualElement) -> list[int]:
        """Returns a list of marriage years (as integers) for an individual
        :type individual: IndividualElement
        :rtype: list of int
        """
    def get_marriages(self, individual: IndividualElement) -> list[tuple[Any, ...]]:
        """Returns a list of marriages of an individual formatted as a tuple (`str` date, `str` place)
        :type individual: IndividualElement
        :rtype: list of tuple
        """
    def get_parents(self, individual: IndividualElement, parent_type: str = "ALL") -> list[IndividualElement]:
        """Return elements corresponding to parents of an individual

        Optional parent_type. Default "ALL" returns all parents. "NAT" can be
        used to specify only natural (genetic) parents.

        :type individual: IndividualElement
        :type parent_type: str
        :rtype: list of IndividualElement
        """
    def get_root_child_elements(self) -> list[Element]:
        """Returns a list of logical records in the GEDCOM file

        By default, elements are in the same order as they appeared in the file.

        :rtype: list of Element
        """
    def get_root_element(self) -> RootElement:
        """Returns a virtual root element containing all logical records as children

        When printed, this element converts to an empty string.

        :rtype: RootElement
        """
    def invalidate_cache(self) -> None:
        """Empties the element list and dictionary to cause `gedcom.parser.Parser.get_element_list()`
        and `gedcom.parser.Parser.get_element_dictionary()` to return updated data.

        The update gets deferred until each of the methods actually gets called.
        """
    def marriage_range_match(self, individual: IndividualElement, from_year: int, to_year: int) -> bool:
        """Check if one of the marriage years of an individual is in a given range. Years are integers.
        :type individual: IndividualElement
        :type from_year: int
        :type to_year: int
        :rtype: bool
        """
    def marriage_year_match(self, individual: IndividualElement, year: int) -> bool:
        """Checks if one of the marriage years of an individual matches the supplied year. Year is an integer.
        :type individual: IndividualElement
        :type year: int
        :rtype: bool
        """
    def parse(self, gedcom_stream: Iterable[bytes], strict: bool | None = None) -> None:
        """Parses a stream, or an array of lines, as GEDCOM 5.5 formatted data
        :type gedcom_stream: a file stream, or bytes array of lines with new line at the end
        :type strict: bool
        """
    def parse_file(self, file_path: str, strict: bool | None = None) -> None:
        """Opens and parses a file, from the given file path, as GEDCOM 5.5 formatted data
        :type file_path: str
        :type strict: bool
        """
    def print_gedcom(self) -> None:
        """Write GEDCOM data to stdout"""
    def save_gedcom(self, open_file: IO[str]) -> None:
        """Save GEDCOM data to a file
        :type open_file: file
        """
    @property
    def source(self) -> SourceInfo | None:
        """What the last document's header said about itself

        `None` unless the parser was built with `load_from_source=True`.

        :rtype: SourceInfo or None
        """
    def validate(self, version: str | None = None) -> list[Finding]:
        """Checks the parsed document against the GEDCOM release it declares

        :type version: str
        :rtype: list of Finding
        """

def _tag_constants() -> dict[str, str]: ...
