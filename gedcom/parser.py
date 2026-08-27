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
Module containing the actual `gedcom.parser.Parser` used to generate elements - out of each line -
which can in return be manipulated.
"""

from gedcom._gedcom import GedcomFormatViolationError, ParseError, Parser
import gedcom.tags

FAMILY_MEMBERS_TYPE_ALL = "ALL"
FAMILY_MEMBERS_TYPE_CHILDREN = gedcom.tags.GEDCOM_TAG_CHILD
FAMILY_MEMBERS_TYPE_HUSBAND = gedcom.tags.GEDCOM_TAG_HUSBAND
FAMILY_MEMBERS_TYPE_PARENTS = "PARENTS"
FAMILY_MEMBERS_TYPE_WIFE = gedcom.tags.GEDCOM_TAG_WIFE

def __getattr__(name):
    """Re-export `ParserConfig` without importing pydantic up front."""
    if name == "ParserConfig":
        from gedcom.config import ParserConfig

        return ParserConfig
    raise AttributeError("module %r has no attribute %r" % (__name__, name))


__all__ = [
    "FAMILY_MEMBERS_TYPE_ALL",
    "FAMILY_MEMBERS_TYPE_CHILDREN",
    "FAMILY_MEMBERS_TYPE_HUSBAND",
    "FAMILY_MEMBERS_TYPE_PARENTS",
    "FAMILY_MEMBERS_TYPE_WIFE",
    "GedcomFormatViolationError",
    "ParseError",
    "Parser",
    "ParserConfig",
]
