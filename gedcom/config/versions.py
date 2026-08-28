# -*- coding: utf-8 -*-

# Python GEDCOM Parser
#
# Copyright (C) 2018 Damon Brodie (damon.brodie at gmail.com)
# Copyright (C) 2018-2019 Nicklas Reincke (contact at reynke.com)
# Copyright (C) 2016 Andreas Oberritter
# Copyright (C) 2012 Madeleine Price Ball
# Copyright (C) 2005 Daniel Zappala (zappala at cs.byu.edu)
# Copyright (C) 2005 Brigham Young University
#
# This program is free software; you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation; either version 2 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License along
# with this program; if not, write to the Free Software Foundation, Inc.,
# 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.
#
# Further information about the license: http://www.gnu.org/licenses/gpl-2.0.html

"""
How the GEDCOM 5.x releases differ, and what that means for a line parser."""

from typing import Dict, FrozenSet

__all__ = [
    "VERSIONS",
    "VERSION_CHARACTER_SETS",
    "LINE_LIMITS",
    "TAGS_INTRODUCED",
    "TAGS_REMOVED",
    "XREF_MINIMUM",
    "version_note",
    "line_limit",
]

VERSIONS = ("5.5", "5.5.1", "5.5.5")

VERSION_CHARACTER_SETS: Dict[str, FrozenSet[str]] = {
    "5.5": frozenset({"ANSEL", "UNICODE", "ASCII"}),
    "5.5.1": frozenset({"ANSEL", "UTF-8", "UNICODE", "ASCII"}),
    "5.5.5": frozenset({"UTF-8", "UNICODE"}),
}

LINE_LIMITS: Dict[str, int] = {
    "5.5": 255,
    "5.5.1": 255,
    "5.5.5": 32767,
}

DEFAULT_LINE_LIMIT = 255

TAGS_INTRODUCED: Dict[str, FrozenSet[str]] = {
    "5.5.1": frozenset({"EMAIL", "FAX", "WWW", "FONE", "ROMN",
                        "MAP", "LATI", "LONG", "FACT"}),
}

TAGS_REMOVED: Dict[str, FrozenSet[str]] = {
    "5.5.1": frozenset({"BLOB", "LEGA"}),
}

XREF_MINIMUM: Dict[str, int] = {
    "5.5": 1,
    "5.5.1": 1,
    "5.5.5": 3,
}


def version_note(version, character_set):
    """Say so when a header declares a version and a character set that never
    went together.

    :type version: str or None
    :type character_set: str or None
    :rtype: str or None
    """
    if not version or not character_set:
        return None

    allowed = VERSION_CHARACTER_SETS.get(version.strip())
    if allowed is None:
        return None

    declared = character_set.strip().upper()
    if declared in allowed:
        return None

    return (
        "the header declares GEDCOM %s with CHAR %s, which that release does "
        "not allow (%s); reading it as the character set says"
        % (version.strip(), character_set.strip(), ", ".join(sorted(allowed)))
    )


def line_limit(version):
    """The longest line the named release allows.

    :type version: str or None
    :rtype: int
    """
    if not version:
        return DEFAULT_LINE_LIMIT
    return LINE_LIMITS.get(version.strip(), DEFAULT_LINE_LIMIT)
