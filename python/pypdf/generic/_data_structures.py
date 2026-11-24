"""pypdf port stub

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf.generic._data_structures``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

__author__ = 'Mathieu Fenniak'
__author_email__ = 'biziqe@mathieu.fenniak.net'
import inspect
import logging
import os
import re
import sys
from collections.abc import Callable, Iterable, Sequence
from functools import cache
from io import BytesIO
from typing import Any, NamedTuple, Optional, Union, cast
from .. import get_configuration
from .._protocols import PdfReaderProtocol, PdfWriterProtocol, XmpInformationProtocol
from .._utils import WHITESPACES, BinaryStreamType, StreamType, logger_warning, read_non_whitespace, read_until_regex, read_until_whitespace, skip_over_comment, skip_over_comments
from ..constants import CheckboxRadioButtonAttributes, FieldDictionaryAttributes, FilterTypes, OutlineFontFlag, StreamAttributes, TypArguments, TypFitArguments
from ..errors import STREAM_TRUNCATED_PREMATURELY, LimitReachedError, PdfReadError, PdfStreamError
from ._base import BooleanObject, ByteStringObject, FloatObject, IndirectObject, NameObject, NullObject, NumberObject, PdfObject, TextStringObject, is_null_or_none
from ._fit import Fit
from ._utils import StreamParsingBudget, read_hex_string_from_stream, read_string_from_stream
if sys.version_info >= (3, 11):
    from typing import Self
else:
    from typing_extensions import Self
logger = None
IndirectPattern = None
_SCALAR_TYPES = None
class ArrayObject(list, PdfObject):
    """Stub for pypdf.generic._data_structures.ArrayObject."""
    def replicate(self, *args, **kwargs):
        return None  # inert stub
    def clone(self, *args, **kwargs):
        return None  # inert stub
    def hash_bin(self, *args, **kwargs):
        return None  # inert stub
    def items(self, *args, **kwargs):
        return None  # inert stub
    def _to_lst(self, *args, **kwargs):
        return None  # inert stub
    def __add__(self, *args, **kwargs):
        return None  # inert stub
    def __iadd__(self, *args, **kwargs):
        return None  # inert stub
    def __isub__(self, *args, **kwargs):
        return None  # inert stub
    def write_to_stream(self, *args, **kwargs):
        return None  # inert stub
    @staticmethod
    def _check_limits(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def read_from_stream(*args, **kwargs):
        return None  # inert stub
def _requires_constructor_arguments(*args, **kwargs):
    return None  # inert stub: pypdf.generic._data_structures._requires_constructor_arguments
class DictionaryObject(dict, PdfObject):
    """Stub for pypdf.generic._data_structures.DictionaryObject."""
    def _new_for_clone(self, *args, **kwargs):
        return None  # inert stub
    def replicate(self, *args, **kwargs):
        return None  # inert stub
    def clone(self, *args, **kwargs):
        return None  # inert stub
    def _clone(self, *args, **kwargs):
        return None  # inert stub
    def hash_bin(self, *args, **kwargs):
        return None  # inert stub
    def raw_get(self, *args, **kwargs):
        return None  # inert stub
    def get_inherited(self, *args, **kwargs):
        return None  # inert stub
    def __setitem__(self, *args, **kwargs):
        return None  # inert stub
    def setdefault(self, *args, **kwargs):
        return None  # inert stub
    def __getitem__(self, *args, **kwargs):
        return None  # inert stub
    @property
    def xmp_metadata(self):
        return None  # inert stub
    def write_to_stream(self, *args, **kwargs):
        return None  # inert stub
    @classmethod
    def _get_next_object_position(cls, *args, **kwargs):
        return None  # inert stub
    @classmethod
    def _read_unsized_from_stream(cls, *args, **kwargs):
        return None  # inert stub
    @staticmethod
    def read_from_stream(*args, **kwargs):
        return None  # inert stub
    @staticmethod
    def _read_stream_data(*args, **kwargs):
        return None  # inert stub
class TreeObject(DictionaryObject):
    """Stub for pypdf.generic._data_structures.TreeObject."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def has_children(self, *args, **kwargs):
        return None  # inert stub
    def __iter__(self, *args, **kwargs):
        return None  # inert stub
    def children(self, *args, **kwargs):
        return None  # inert stub
    def add_child(self, *args, **kwargs):
        return None  # inert stub
    def inc_parent_counter_default(self, *args, **kwargs):
        return None  # inert stub
    def inc_parent_counter_outline(self, *args, **kwargs):
        return None  # inert stub
    def insert_child(self, *args, **kwargs):
        return None  # inert stub
    def _remove_node_from_tree(self, *args, **kwargs):
        return None  # inert stub
    def remove_child(self, *args, **kwargs):
        return None  # inert stub
    def remove_from_tree(self, *args, **kwargs):
        return None  # inert stub
    def empty_tree(self, *args, **kwargs):
        return None  # inert stub
def _reset_node_tree_relationship(*args, **kwargs):
    return None  # inert stub: pypdf.generic._data_structures._reset_node_tree_relationship
class _FontFileCharacterMap:
    """Stub for pypdf.generic._data_structures._FontFileCharacterMap."""
    digest = None
    map_dict = None
    int_entry = None
class StreamObject(DictionaryObject):
    """Stub for pypdf.generic._data_structures.StreamObject."""
    _font_file_character_map = None
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def replicate(self, *args, **kwargs):
        return None  # inert stub
    def _clone(self, *args, **kwargs):
        return None  # inert stub
    def hash_bin(self, *args, **kwargs):
        return None  # inert stub
    def get_data(self, *args, **kwargs):
        return None  # inert stub
    def set_data(self, *args, **kwargs):
        return None  # inert stub
    def hash_value_data(self, *args, **kwargs):
        return None  # inert stub
    def write_to_stream(self, *args, **kwargs):
        return None  # inert stub
    @staticmethod
    def initialize_from_dictionary(*args, **kwargs):
        return None  # inert stub
    def flate_encode(self, *args, **kwargs):
        return None  # inert stub
    def decode_as_image(self, *args, **kwargs):
        return None  # inert stub
class DecodedStreamObject(StreamObject):
    """Stub for pypdf.generic._data_structures.DecodedStreamObject."""
    pass
class EncodedStreamObject(StreamObject):
    """Stub for pypdf.generic._data_structures.EncodedStreamObject."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def hash_value_data(self, *args, **kwargs):
        return None  # inert stub
    def get_data(self, *args, **kwargs):
        return None  # inert stub
    def set_data(self, *args, **kwargs):
        return None  # inert stub
CONTENT_STREAM_ARRAY_MAX_LENGTH = 10000
class ContentStream(DecodedStreamObject):
    """Stub for pypdf.generic._data_structures.ContentStream."""
    _OPERATOR_LENGTH_LIMIT = 128
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def replicate(self, *args, **kwargs):
        return None  # inert stub
    def clone(self, *args, **kwargs):
        return None  # inert stub
    def _clone(self, *args, **kwargs):
        return None  # inert stub
    @staticmethod
    def _check_limits(*args, **kwargs):
        return None  # inert stub
    def _parse_content_stream(self, *args, **kwargs):
        return None  # inert stub
    def get_data(self, *args, **kwargs):
        return None  # inert stub
    def set_data(self, *args, **kwargs):
        return None  # inert stub
    @property
    def operations(self):
        return None  # inert stub
    def operations(self, *args, **kwargs):
        return None  # inert stub
    def isolate_graphics_state(self, *args, **kwargs):
        return None  # inert stub
    def write_to_stream(self, *args, **kwargs):
        return None  # inert stub
_MAX_LEADING_COMMENTS = 1000
def read_object(*args, **kwargs):
    return None  # inert stub: pypdf.generic._data_structures.read_object
class Field(TreeObject):
    """Stub for pypdf.generic._data_structures.Field."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    @property
    def field_type(self):
        return None  # inert stub
    @property
    def parent(self):
        return None  # inert stub
    @property
    def kids(self):
        return None  # inert stub
    @property
    def name(self):
        return None  # inert stub
    @property
    def alternate_name(self):
        return None  # inert stub
    @property
    def mapping_name(self):
        return None  # inert stub
    @property
    def flags(self):
        return None  # inert stub
    @property
    def value(self):
        return None  # inert stub
    @property
    def default_value(self):
        return None  # inert stub
    @property
    def additional_actions(self):
        return None  # inert stub
class Destination(TreeObject):
    """Stub for pypdf.generic._data_structures.Destination."""
    node = None
    def remove_from_tree(self, *args, **kwargs):
        return None  # inert stub
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    @property
    def dest_array(self):
        return None  # inert stub
    def write_to_stream(self, *args, **kwargs):
        return None  # inert stub
    @property
    def title(self):
        return None  # inert stub
    @property
    def page(self):
        return None  # inert stub
    @property
    def typ(self):
        return None  # inert stub
    @property
    def zoom(self):
        return None  # inert stub
    @property
    def left(self):
        return None  # inert stub
    @property
    def right(self):
        return None  # inert stub
    @property
    def top(self):
        return None  # inert stub
    @property
    def bottom(self):
        return None  # inert stub
    @property
    def color(self):
        return None  # inert stub
    @property
    def font_format(self):
        return None  # inert stub
    @property
    def outline_count(self):
        return None  # inert stub
