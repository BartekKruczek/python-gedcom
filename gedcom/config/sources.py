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
What the program that wrote a document does differently."""

from typing import Dict, Optional

from pydantic import BaseModel, ConfigDict

__all__ = ["SourceProfile", "PROFILES", "profile_for", "encoding_for"]


class SourceProfile(BaseModel):
    """How one program's exports deviate from GEDCOM 5.5."""

    model_config = ConfigDict(frozen=True, extra="forbid")

    name: str
    """The program's name, as a person would write it."""

    unterminated_final_line: bool = False
    """The last line of the file carries no terminator."""

PROFILES: Dict[str, SourceProfile] = {
    "MYHERITAGE": SourceProfile(
        name="MyHeritage Family Tree Builder",
        unterminated_final_line=True,
    ),
    "ANCESTRY": SourceProfile(name="Ancestry Member Trees"),
    "ANSTFILE": SourceProfile(name="FamilySearch Ancestral File"),
    "FTM": SourceProfile(name="Family Tree Maker"),
    "FTW": SourceProfile(name="Family Tree Maker for Windows"),
    "GENI.COM": SourceProfile(name="Geni.com"),
    "GRAMPS": SourceProfile(name="Gramps"),
    "LEGACY": SourceProfile(name="Legacy Family Tree"),
    "PAF": SourceProfile(name="Personal Ancestral File"),
    "ROOTSMAGIC": SourceProfile(name="RootsMagic"),
}


def profile_for(system: Optional[str]) -> Optional[SourceProfile]:
    """The profile for a `HEAD.SOUR` value, or `None` if it is not a known one.

    :type system: str or None
    :rtype: SourceProfile or None
    """
    if not system:
        return None
    return PROFILES.get(system.strip().upper())


CHARACTER_SETS = {
    "UTF-8": "utf-8-sig",
    "UTF8": "utf-8-sig",
    "UNICODE": "utf-16",
    "ASCII": "ascii",
}

UNSUPPORTED_CHARACTER_SETS = {
    "ANSEL": "ANSEL, the historic GEDCOM character set, has no codec in the "
             "standard library",
    "ANSI": "ANSI names a different code page in every locale",
}


def encoding_for(character_set):
    """The codec a `HEAD.CHAR` value calls for.

    :type character_set: str or None
    :rtype: str or None
    """
    if not character_set:
        return None
    key = character_set.strip().upper()
    if key in CHARACTER_SETS:
        return CHARACTER_SETS[key]
    if key in UNSUPPORTED_CHARACTER_SETS:
        raise ValueError(
            "the document declares %s: %s. Set the encoding explicitly, for "
            "example ParserConfig(encoding=\"cp1250\")."
            % (character_set.strip(), UNSUPPORTED_CHARACTER_SETS[key])
        )
    return None

