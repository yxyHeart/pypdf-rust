"""Representation and utils for ranges of PDF file pages.

Copyright (c) 2014, Steve Witham <switham_github@mac-guyver.com>.
All rights reserved. This software is available under a BSD license;
see https://github.com/py-pdf/pypdf/blob/main/LICENSE

Rust-backed module: the implementation lives in ``crates/pypdf-py/src/pagerange.rs``.
"""

from __future__ import annotations

from pypdf._pypdf import PAGE_RANGE_RE, PageRange, parse_filename_page_ranges

_INT_RE = r"(0|-?[1-9]\d*)"  # A decimal int, don't allow "-0".

PAGE_RANGE_ALL = PageRange(":")  # The range of all pages.

PageRangeSpec = str | PageRange | tuple[int, int] | tuple[int, int, int] | list[int]
