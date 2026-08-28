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
Everything that decides how `gedcom.parser.Parser` reads a document.

`ParserConfig`, here, is what a caller sets. The two submodules hold what the
parser knows rather than what it is told:

- `gedcom.config.sources` -- the program that wrote a file, and how its exports
  deviate from the standard.
- `gedcom.config.versions` -- how the GEDCOM 5.x releases differ, and which of
  those differences a line parser can act on.
"""

import codecs
from typing import Literal

from pydantic import BaseModel, ConfigDict, field_validator

__all__ = ["ParserConfig"]


class ParserConfig(BaseModel):
    """Settings a `gedcom.parser.Parser` is built with."""

    model_config = ConfigDict(frozen=True, extra="forbid")

    strict: bool = True
    """Reject any line that violates GEDCOM 5.5."""

    encoding: str = "utf-8-sig"
    """Codec used to decode a document."""

    on_error: Literal["raise", "collect"] = "raise"
    """What to do with a line the parser cannot accept."""

    load_from_source: bool = False
    """Take the encoding and the known quirks from the document's own header."""

    @field_validator("encoding")
    @classmethod
    def _codec_must_exist(cls, value: str) -> str:
        """Fail here rather than part-way through a parse."""
        try:
            codecs.lookup(value)
        except LookupError:
            raise ValueError("unknown encoding: %r" % value) from None
        return value
