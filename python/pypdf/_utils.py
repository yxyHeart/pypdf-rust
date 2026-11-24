"""Utility functions for PDF library.

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf._utils``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

__author__ = 'Mathieu Fenniak'
__author_email__ = 'biziqe@mathieu.fenniak.net'
import functools
import inspect
import logging
import os
import re
import sys
import warnings
from collections.abc import Callable
from dataclasses import dataclass
from datetime import datetime, timezone
from io import DEFAULT_BUFFER_SIZE
from os import SEEK_CUR
from re import Pattern
from typing import IO, Any, NoReturn, ParamSpec, TypeAlias, TypeVar, cast
if sys.version_info >= (3, 11):
    from typing import Self
else:
    from typing_extensions import Self
from .errors import STREAM_TRUNCATED_PREMATURELY, DeprecationError, LimitReachedError, PdfStreamError
TransformationMatrixType = None
CompressedTransformationMatrix = None
StreamType = None
BinaryStreamType = None
StrByteType = None
def parse_iso8824_date(*args, **kwargs):
    return None  # inert stub: pypdf._utils.parse_iso8824_date
def format_iso8824_date(*args, **kwargs):
    return None  # inert stub: pypdf._utils.format_iso8824_date
def _get_max_pdf_version_header(*args, **kwargs):
    return None  # inert stub: pypdf._utils._get_max_pdf_version_header
WHITESPACES = ()
WHITESPACES_AS_BYTES = None
WHITESPACES_AS_REGEXP = None
def read_until_whitespace(*args, **kwargs):
    return None  # inert stub: pypdf._utils.read_until_whitespace
def read_non_whitespace(*args, **kwargs):
    return None  # inert stub: pypdf._utils.read_non_whitespace
def skip_over_whitespace(*args, **kwargs):
    return None  # inert stub: pypdf._utils.skip_over_whitespace
def check_if_whitespace_only(*args, **kwargs):
    return None  # inert stub: pypdf._utils.check_if_whitespace_only
NEUTRAL_CHARACTER_RANGES = ()
def is_char_neutral(*args, **kwargs):
    return None  # inert stub: pypdf._utils.is_char_neutral
RTL_CHARACTER_RANGES = ()
def is_char_rtl(*args, **kwargs):
    return None  # inert stub: pypdf._utils.is_char_rtl
def skip_over_comment(*args, **kwargs):
    return None  # inert stub: pypdf._utils.skip_over_comment
def skip_over_comments(*args, **kwargs):
    return None  # inert stub: pypdf._utils.skip_over_comments
def read_until_regex(*args, **kwargs):
    return None  # inert stub: pypdf._utils.read_until_regex
def read_block_backwards(*args, **kwargs):
    return None  # inert stub: pypdf._utils.read_block_backwards
def read_previous_line(*args, **kwargs):
    return None  # inert stub: pypdf._utils.read_previous_line
def matrix_multiply(*args, **kwargs):
    return None  # inert stub: pypdf._utils.matrix_multiply
def mark_location(*args, **kwargs):
    return None  # inert stub: pypdf._utils.mark_location
def deprecate(*args, **kwargs):
    return None  # inert stub: pypdf._utils.deprecate
def deprecation(*args, **kwargs):
    return None  # inert stub: pypdf._utils.deprecation
def deprecate_with_replacement(*args, **kwargs):
    return None  # inert stub: pypdf._utils.deprecate_with_replacement
def deprecation_with_replacement(*args, **kwargs):
    return None  # inert stub: pypdf._utils.deprecation_with_replacement
_P = None
_R = None
def deprecate_positional_arguments(*args, **kwargs):
    return None  # inert stub: pypdf._utils.deprecate_positional_arguments
def deprecate_no_replacement(*args, **kwargs):
    return None  # inert stub: pypdf._utils.deprecate_no_replacement
def deprecation_no_replacement(*args, **kwargs):
    return None  # inert stub: pypdf._utils.deprecation_no_replacement
def logger_error(*args, **kwargs):
    return None  # inert stub: pypdf._utils.logger_error
def logger_warning(*args, **kwargs):
    return None  # inert stub: pypdf._utils.logger_warning
def rename_kwargs(*args, **kwargs):
    return None  # inert stub: pypdf._utils.rename_kwargs
def _human_readable_bytes(*args, **kwargs):
    return None  # inert stub: pypdf._utils._human_readable_bytes
def get_stream_size(*args, **kwargs):
    return None  # inert stub: pypdf._utils.get_stream_size
def get_stream_data(*args, **kwargs):
    return None  # inert stub: pypdf._utils.get_stream_data
class classproperty:
    """Stub for pypdf._utils.classproperty."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def __get__(self, *args, **kwargs):
        return None  # inert stub
    def getter(self, *args, **kwargs):
        return None  # inert stub
class File:
    """Stub for pypdf._utils.File."""
    name = None
    data = None
    indirect_reference = None
    def __str__(self, *args, **kwargs):
        return ""  # inert stub
    def __repr__(self, *args, **kwargs):
        return ""  # inert stub
class Version:
    """Stub for pypdf._utils.Version."""
    COMPONENT_PATTERN = None
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def _parse_version(self, *args, **kwargs):
        return None  # inert stub
    def __eq__(self, *args, **kwargs):
        return None  # inert stub
    def __hash__(self, *args, **kwargs):
        return None  # inert stub
    def __lt__(self, *args, **kwargs):
        return None  # inert stub
class _TraversalState:
    """Stub for pypdf._utils._TraversalState."""
    entry_count = None
    has_logged = None
