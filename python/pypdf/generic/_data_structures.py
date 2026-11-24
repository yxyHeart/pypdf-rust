# Copyright (c) 2006, Mathieu Fenniak
# All rights reserved.
#
# Redistribution and use in source and binary forms, with or without
# modification, are permitted provided that the following conditions are
# met:
#
# * Redistributions of source code must retain the above copyright notice,
# this list of conditions and the following disclaimer.
# * Redistributions in binary form must reproduce the above copyright notice,
# this list of conditions and the following disclaimer in the documentation
# and/or other materials provided with the distribution.
# * The name of the author may not be used to endorse or promote products
# derived from this software without specific prior written permission.
#
# THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
# AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
# IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
# ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT OWNER OR CONTRIBUTORS BE
# LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
# CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
# SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
# INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
# CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
# ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
# POSSIBILITY OF SUCH DAMAGE.

"""Rust-backed port of ``pypdf/generic/_data_structures.py`` (pypdf 7.1.1).

The class hierarchy, class attributes, properties and decorators are kept
verbatim from upstream so that the observable Python object model (MRO,
``isinstance``, subclassing, ``sys.settrace`` call graphs) is unchanged.  Every
method body delegates to the Rust implementation in
``crates/pypdf-py/src/modules/data_structures_mod.rs`` (reached through
``pypdf._pypdf``), which contains all of the actual logic.
"""

__author__ = "Mathieu Fenniak"
__author_email__ = "biziqe@mathieu.fenniak.net"

import inspect
import logging
import os
import re
import sys
from collections.abc import Callable, Iterable, Sequence
from functools import cache
from io import BytesIO
from typing import Any, NamedTuple, Optional, Union, cast

from pypdf._pypdf import (
    datastructures_arrayobject_add as _arrayobject_add,
    datastructures_arrayobject_check_limits as _arrayobject_check_limits,
    datastructures_arrayobject_clone as _arrayobject_clone,
    datastructures_arrayobject_hash_bin as _arrayobject_hash_bin,
    datastructures_arrayobject_iadd as _arrayobject_iadd,
    datastructures_arrayobject_isub as _arrayobject_isub,
    datastructures_arrayobject_items as _arrayobject_items,
    datastructures_arrayobject_read_from_stream as _arrayobject_read_from_stream,
    datastructures_arrayobject_replicate as _arrayobject_replicate,
    datastructures_arrayobject_to_lst as _arrayobject_to_lst,
    datastructures_arrayobject_write_to_stream as _arrayobject_write_to_stream,
    datastructures_contentstream_check_limits as _contentstream_check_limits,
    datastructures_contentstream_clone as _contentstream_clone,
    datastructures_contentstream_clone_inner as _contentstream_clone_inner,
    datastructures_contentstream_get_data as _contentstream_get_data,
    datastructures_contentstream_isolate_graphics_state as _contentstream_isolate_graphics_state,
    datastructures_contentstream_new as _contentstream_new,
    datastructures_contentstream_operations as _contentstream_operations,
    datastructures_contentstream_parse_content_stream as _contentstream_parse_content_stream,
    datastructures_contentstream_replicate as _contentstream_replicate,
    datastructures_contentstream_set_data as _contentstream_set_data,
    datastructures_contentstream_set_operations as _contentstream_set_operations,
    datastructures_contentstream_write_to_stream as _contentstream_write_to_stream,
    datastructures_destination_bottom as _destination_bottom,
    datastructures_destination_color as _destination_color,
    datastructures_destination_dest_array as _destination_dest_array,
    datastructures_destination_font_format as _destination_font_format,
    datastructures_destination_left as _destination_left,
    datastructures_destination_new as _destination_new,
    datastructures_destination_outline_count as _destination_outline_count,
    datastructures_destination_page as _destination_page,
    datastructures_destination_remove_from_tree as _destination_remove_from_tree,
    datastructures_destination_right as _destination_right,
    datastructures_destination_title as _destination_title,
    datastructures_destination_top as _destination_top,
    datastructures_destination_typ as _destination_typ,
    datastructures_destination_write_to_stream as _destination_write_to_stream,
    datastructures_destination_zoom as _destination_zoom,
    datastructures_dictionaryobject_clone as _dictionaryobject_clone,
    datastructures_dictionaryobject_clone_inner as _dictionaryobject_clone_inner,
    datastructures_dictionaryobject_get_inherited as _dictionaryobject_get_inherited,
    datastructures_dictionaryobject_get_next_object_position as _dictionaryobject_get_next_object_position,
    datastructures_dictionaryobject_getitem as _dictionaryobject_getitem,
    datastructures_dictionaryobject_hash_bin as _dictionaryobject_hash_bin,
    datastructures_dictionaryobject_new_for_clone as _dictionaryobject_new_for_clone,
    datastructures_dictionaryobject_raw_get as _dictionaryobject_raw_get,
    datastructures_dictionaryobject_read_from_stream as _dictionaryobject_read_from_stream,
    datastructures_dictionaryobject_read_stream_data as _dictionaryobject_read_stream_data,
    datastructures_dictionaryobject_read_unsized_from_stream as _dictionaryobject_read_unsized_from_stream,
    datastructures_dictionaryobject_replicate as _dictionaryobject_replicate,
    datastructures_dictionaryobject_setdefault as _dictionaryobject_setdefault,
    datastructures_dictionaryobject_setitem as _dictionaryobject_setitem,
    datastructures_dictionaryobject_write_to_stream as _dictionaryobject_write_to_stream,
    datastructures_dictionaryobject_xmp_metadata as _dictionaryobject_xmp_metadata,
    datastructures_encodedstreamobject_get_data as _encodedstreamobject_get_data,
    datastructures_encodedstreamobject_hash_value_data as _encodedstreamobject_hash_value_data,
    datastructures_encodedstreamobject_set_data as _encodedstreamobject_set_data,
    datastructures_field_additional_actions as _field_additional_actions,
    datastructures_field_alternate_name as _field_alternate_name,
    datastructures_field_default_value as _field_default_value,
    datastructures_field_field_type as _field_field_type,
    datastructures_field_flags as _field_flags,
    datastructures_field_kids as _field_kids,
    datastructures_field_mapping_name as _field_mapping_name,
    datastructures_field_name as _field_name,
    datastructures_field_new as _field_new,
    datastructures_field_parent as _field_parent,
    datastructures_field_value as _field_value,
    datastructures_read_object as _read_object,
    datastructures_requires_constructor_arguments as _requires_constructor_arguments_impl,
    datastructures_reset_node_tree_relationship as _reset_node_tree_relationship,
    datastructures_streamobject_clone_inner as _streamobject_clone_inner,
    datastructures_streamobject_decode_as_image as _streamobject_decode_as_image,
    datastructures_streamobject_flate_encode as _streamobject_flate_encode,
    datastructures_streamobject_get_data as _streamobject_get_data,
    datastructures_streamobject_hash_bin as _streamobject_hash_bin,
    datastructures_streamobject_hash_value_data as _streamobject_hash_value_data,
    datastructures_streamobject_initialize_from_dictionary as _streamobject_initialize_from_dictionary,
    datastructures_streamobject_replicate as _streamobject_replicate,
    datastructures_streamobject_set_data as _streamobject_set_data,
    datastructures_streamobject_write_to_stream as _streamobject_write_to_stream,
    datastructures_treeobject_add_child as _treeobject_add_child,
    datastructures_treeobject_children_next as _treeobject_children_next,
    datastructures_treeobject_children_start as _treeobject_children_start,
    datastructures_treeobject_empty_tree as _treeobject_empty_tree,
    datastructures_treeobject_has_children as _treeobject_has_children,
    datastructures_treeobject_inc_parent_counter_default as _treeobject_inc_parent_counter_default,
    datastructures_treeobject_inc_parent_counter_outline as _treeobject_inc_parent_counter_outline,
    datastructures_treeobject_insert_child as _treeobject_insert_child,
    datastructures_treeobject_iter as _treeobject_iter,
    datastructures_treeobject_remove_child as _treeobject_remove_child,
    datastructures_treeobject_remove_from_tree as _treeobject_remove_from_tree,
    datastructures_treeobject_remove_node_from_tree as _treeobject_remove_node_from_tree,
)

from .. import get_configuration
from .._protocols import PdfReaderProtocol, PdfWriterProtocol, XmpInformationProtocol
from .._utils import (
    WHITESPACES,
    BinaryStreamType,
    StreamType,
    logger_warning,
    read_non_whitespace,
    read_until_regex,
    read_until_whitespace,
    skip_over_comment,
    skip_over_comments,
)
from ..constants import (
    CheckboxRadioButtonAttributes,
    FieldDictionaryAttributes,
    FilterTypes,
    OutlineFontFlag,
    StreamAttributes,
    TypArguments,
    TypFitArguments,
)
from ..errors import STREAM_TRUNCATED_PREMATURELY, LimitReachedError, PdfReadError, PdfStreamError
from ._base import (
    BooleanObject,
    ByteStringObject,
    FloatObject,
    IndirectObject,
    NameObject,
    NullObject,
    NumberObject,
    PdfObject,
    TextStringObject,
    is_null_or_none,
)
from ._fit import Fit
from ._utils import StreamParsingBudget, read_hex_string_from_stream, read_string_from_stream

if sys.version_info >= (3, 11):
    from typing import Self
else:
    from typing_extensions import Self

logger = logging.getLogger(__name__)

IndirectPattern = re.compile(rb"[+-]?(\d+)\s+(\d+)\s+R[^a-zA-Z]")

# Direct objects of these types are neither streams nor indirect objects.
# Large arrays consist mostly of them, for example in the /ParentTree of the
# structure tree, so the clone loops check the exact type first and skip the
# slower isinstance() checks.
_SCALAR_TYPES = frozenset(
    (BooleanObject, ByteStringObject, FloatObject, NameObject, NullObject, NumberObject, TextStringObject)
)

# Sentinel used by the TreeObject.children() generator trampoline (the traversal
# state machine itself lives in Rust).
_DATA_STRUCTURES_STOP = object()


class ArrayObject(list[Any], PdfObject):
    def replicate(
        self,
        pdf_dest: PdfWriterProtocol,
    ) -> "ArrayObject":
        return cast("ArrayObject", _arrayobject_replicate(self, pdf_dest))

    def clone(
        self,
        pdf_dest: PdfWriterProtocol,
        force_duplicate: bool = False,
        ignore_fields: Sequence[str | int] | None = (),
    ) -> "ArrayObject":
        """Clone object into pdf_dest."""
        return cast(
            "ArrayObject",
            _arrayobject_clone(self, pdf_dest, force_duplicate, ignore_fields),
        )

    def hash_bin(self) -> int:
        """
        Used to detect modified object.

        Returns:
            Hash considering type and value.

        """
        return _arrayobject_hash_bin(self)

    def items(self) -> Iterable[Any]:
        """Emulate DictionaryObject.items for a list (index, object)."""
        return _arrayobject_items(self)

    def _to_lst(self, lst: Any) -> list[Any]:
        # Convert to list, internal
        return _arrayobject_to_lst(self, lst)

    def __add__(self, lst: Any) -> "ArrayObject":
        """
        Allow extension by adding list or add one element only

        Args:
            lst: any list, tuples are extended the list.
            other types(numbers,...) will be appended.
            if str is passed it will be converted into TextStringObject
            or NameObject (if starting with "/")
            if bytes is passed it will be converted into ByteStringObject

        Returns:
            ArrayObject with all elements

        """
        return cast("ArrayObject", _arrayobject_add(self, lst))

    def __iadd__(self, lst: Any) -> Self:
        """
         Allow extension by adding list or add one element only

        Args:
            lst: any list, tuples are extended the list.
            other types(numbers,...) will be appended.
            if str is passed it will be converted into TextStringObject
            or NameObject (if starting with "/")
            if bytes is passed it will be converted into ByteStringObject

        """
        return cast("Self", _arrayobject_iadd(self, lst))

    def __isub__(self, lst: Any) -> Self:
        """Allow to remove items"""
        return cast("Self", _arrayobject_isub(self, lst))

    def write_to_stream(self, stream: StreamType) -> None:
        return _arrayobject_write_to_stream(self, stream)

    @staticmethod
    def _check_limits(*, remaining_elements: int, parsing_budget: StreamParsingBudget | None) -> int:
        return _arrayobject_check_limits(remaining_elements, parsing_budget)

    @staticmethod
    def read_from_stream(
        stream: StreamType,
        pdf: PdfReaderProtocol | None,
        forced_encoding: str | list[str] | dict[int, str] | None = None,
        *,
        parsing_budget: StreamParsingBudget | None = None,
    ) -> "ArrayObject":
        return _arrayobject_read_from_stream(stream, pdf, forced_encoding, parsing_budget)


@cache
def _requires_constructor_arguments(cls: type["DictionaryObject"]) -> bool:
    """Report whether ``cls`` can only be instantiated with arguments."""
    return _requires_constructor_arguments_impl(cls)


class DictionaryObject(dict[Any, Any], PdfObject):
    def _new_for_clone(self) -> "DictionaryObject":
        """Create a blank instance of this class to receive cloned entries.

        Most dictionary classes can be instantiated without arguments, but some
        subclasses - the annotation dictionaries for instance - can only be
        instantiated with arguments. Those arguments are not needed to receive a
        clone, because every entry is copied from the source object afterwards,
        so such classes are instantiated without calling ``__init__``.
        """
        return cast("DictionaryObject", _dictionaryobject_new_for_clone(self))

    def replicate(
        self,
        pdf_dest: PdfWriterProtocol,
    ) -> "DictionaryObject":
        return cast("DictionaryObject", _dictionaryobject_replicate(self, pdf_dest))

    def clone(
        self,
        pdf_dest: PdfWriterProtocol,
        force_duplicate: bool = False,
        ignore_fields: Sequence[str | int] | None = (),
    ) -> "DictionaryObject":
        """Clone object into pdf_dest."""
        return cast(
            "DictionaryObject",
            _dictionaryobject_clone(self, pdf_dest, force_duplicate, ignore_fields),
        )

    def _clone(
        self,
        src: "DictionaryObject",
        pdf_dest: PdfWriterProtocol,
        force_duplicate: bool,
        ignore_fields: Sequence[str | int] | None,
        visited: set[tuple[int, int]],  # (idnum, generation)
    ) -> None:
        """
        Update the object from src.

        Args:
            src: "DictionaryObject":
            pdf_dest:
            force_duplicate:
            ignore_fields:

        """
        return _dictionaryobject_clone_inner(
            self, src, pdf_dest, force_duplicate, ignore_fields, visited
        )

    def hash_bin(self) -> int:
        """
        Used to detect modified object.

        Returns:
            Hash considering type and value.

        """
        return _dictionaryobject_hash_bin(self)

    def raw_get(self, key: Any) -> Any:
        return _dictionaryobject_raw_get(self, key)

    def get_inherited(self, key: str, default: Any = None) -> Any:
        """
        Returns the value of a key or from the parent if not found.
        If not found returns default.

        Args:
            key: string identifying the field to return

            default: default value to return

        Returns:
            Current key or inherited one, otherwise default value.

        """
        return _dictionaryobject_get_inherited(self, key, default)

    def __setitem__(self, key: Any, value: Any) -> Any:
        return _dictionaryobject_setitem(self, key, value)

    def setdefault(self, key: Any, value: Any | None = None) -> Any:
        return _dictionaryobject_setdefault(self, key, value)

    def __getitem__(self, key: Any) -> PdfObject:
        return cast(PdfObject, _dictionaryobject_getitem(self, key))

    @property
    def xmp_metadata(self) -> XmpInformationProtocol | None:
        """
        Retrieve XMP (Extensible Metadata Platform) data relevant to this
        object, if available.

        See Table 347 — Additional entries in a metadata stream dictionary.

        Returns:
          Returns a :class:`~pypdf.xmp.XmpInformation` instance
          that can be used to access XMP metadata from the document. Can also
          return None if no metadata was found on the document root.

        """
        return _dictionaryobject_xmp_metadata(self)

    def write_to_stream(self, stream: StreamType) -> None:
        return _dictionaryobject_write_to_stream(self, stream)

    @classmethod
    def _get_next_object_position(
        cls, position_before: int, position_end: int, generations: list[int], pdf: PdfReaderProtocol
    ) -> int:
        return _dictionaryobject_get_next_object_position(
            cls, position_before, position_end, generations, pdf
        )

    @classmethod
    def _read_unsized_from_stream(
        cls, *, stream: BinaryStreamType, pdf: PdfReaderProtocol, length: int
    ) -> bytes:
        return _dictionaryobject_read_unsized_from_stream(
            cls, stream=stream, pdf=pdf, length=length
        )

    @staticmethod
    def read_from_stream(
        stream: StreamType,
        pdf: PdfReaderProtocol | None,
        forced_encoding: str | list[str] | dict[int, str] | None = None,
        *,
        parsing_budget: StreamParsingBudget | None = None,
    ) -> "DictionaryObject":
        return _dictionaryobject_read_from_stream(stream, pdf, forced_encoding, parsing_budget)

    @staticmethod
    def _read_stream_data(
        *,
        stream: StreamType,
        pdf: PdfReaderProtocol | None,
        data: dict[Any, Any],
    ) -> None:
        return _dictionaryobject_read_stream_data(stream=stream, pdf=pdf, data=data)


class TreeObject(DictionaryObject):
    def __init__(self, dct: DictionaryObject | None = None) -> None:
        DictionaryObject.__init__(self)
        if dct:
            self.update(dct)

    def has_children(self) -> bool:
        return _treeobject_has_children(self)

    def __iter__(self) -> Any:
        return _treeobject_iter(self)

    def children(self) -> Iterable[Any]:
        return _tree_children_generator(self)

    def add_child(self, child: Any, pdf: PdfWriterProtocol) -> None:
        return _treeobject_add_child(self, child, pdf)

    def inc_parent_counter_default(
        self, parent: Union[IndirectObject, "TreeObject", None], n: int
    ) -> None:
        return _treeobject_inc_parent_counter_default(self, parent, n)

    def inc_parent_counter_outline(
        self, parent: Union[IndirectObject, "TreeObject", None], n: int
    ) -> None:
        return _treeobject_inc_parent_counter_outline(self, parent, n)

    def insert_child(
        self,
        child: Any,
        before: Any,
        pdf: PdfWriterProtocol,
        inc_parent_counter: Callable[..., Any] | None = None,
    ) -> IndirectObject:
        return cast(
            IndirectObject,
            _treeobject_insert_child(self, child, before, pdf, inc_parent_counter),
        )

    def _remove_node_from_tree(
        self, *, previous: Any, previous_reference: Any, current: Any, last: Any
    ) -> None:
        """Adjust the pointers of the linked list and tree node count."""
        return _treeobject_remove_node_from_tree(
            self,
            previous=previous,
            previous_reference=previous_reference,
            current=current,
            last=last,
        )

    def remove_child(self, child: Any) -> None:
        return _treeobject_remove_child(self, child)

    def remove_from_tree(self) -> None:
        """Remove the object from the tree it is in."""
        return _treeobject_remove_from_tree(self)

    def empty_tree(self) -> None:
        return _treeobject_empty_tree(self)


def _tree_children_generator(self: "TreeObject") -> Iterable[Any]:
    """Generator trampoline for ``TreeObject.children()``.

    The traversal/cycle-detection state machine lives in Rust
    (``datastructures_treeobject_children_start`` /
    ``datastructures_treeobject_children_next``); this function only yields the
    values it produces so that ``children()`` stays a real generator like
    upstream.
    """
    state = _treeobject_children_start(self)
    if state is None:
        return
    while True:
        child = _treeobject_children_next(state)
        if child is _DATA_STRUCTURES_STOP:
            return
        yield child


class _FontFileCharacterMap(NamedTuple):
    """The character map pypdf._cmap derived from an embedded font program. See #4156."""

    digest: bytes
    """SHA-256 digest of the decoded font data the map was derived from."""
    map_dict: dict[Any, Any]
    int_entry: list[int]


class StreamObject(DictionaryObject):
    _font_file_character_map: _FontFileCharacterMap | None = None
    """
    The character map pypdf._cmap derived from this stream as an embedded font program. Parsing a CFF font
    program with fontTools is expensive, and the same program is reached from every font resource and page
    referencing it. See #4156.
    """

    def __init__(self) -> None:
        self._data: bytes = b""
        self.decoded_self: DecodedStreamObject | None = None

    def replicate(
        self,
        pdf_dest: PdfWriterProtocol,
    ) -> "StreamObject":
        return cast("StreamObject", _streamobject_replicate(self, pdf_dest))

    def _clone(
        self,
        src: DictionaryObject,
        pdf_dest: PdfWriterProtocol,
        force_duplicate: bool,
        ignore_fields: Sequence[str | int] | None,
        visited: set[tuple[int, int]],
    ) -> None:
        """
        Update the object from src.

        Args:
            src:
            pdf_dest:
            force_duplicate:
            ignore_fields:

        """
        return _streamobject_clone_inner(
            self, src, pdf_dest, force_duplicate, ignore_fields, visited
        )

    def hash_bin(self) -> int:
        """
        Used to detect modified object.

        Returns:
            Hash considering type and value.

        """
        return _streamobject_hash_bin(self)

    def get_data(self) -> bytes:
        return _streamobject_get_data(self)

    def set_data(self, data: bytes) -> None:
        return _streamobject_set_data(self, data)

    def hash_value_data(self) -> bytes:
        return _streamobject_hash_value_data(self)

    def write_to_stream(self, stream: StreamType) -> None:
        return _streamobject_write_to_stream(self, stream)

    @staticmethod
    def initialize_from_dictionary(
        data: dict[str, Any]
    ) -> Union["EncodedStreamObject", "DecodedStreamObject"]:
        return cast(
            Union["EncodedStreamObject", "DecodedStreamObject"],
            _streamobject_initialize_from_dictionary(data),
        )

    def flate_encode(self, level: int = -1) -> "EncodedStreamObject":
        return cast("EncodedStreamObject", _streamobject_flate_encode(self, level))

    def decode_as_image(self, pillow_parameters: dict[str, Any] | None = None) -> Any:
        """
        Try to decode the stream object as an image

        Args:
            pillow_parameters: parameters provided to Pillow Image.save() method,
                cf. <https://pillow.readthedocs.io/en/stable/reference/Image.html#PIL.Image.Image.save>

        Returns:
            a PIL image if proper decoding has been found
        Raises:
            Exception: Errors during decoding will be reported.
                It is recommended to catch exceptions to prevent
                stops in your program.

        """
        return _streamobject_decode_as_image(self, pillow_parameters)


class DecodedStreamObject(StreamObject):
    pass


class EncodedStreamObject(StreamObject):
    def __init__(self) -> None:
        self.decoded_self: DecodedStreamObject | None = None

    def hash_value_data(self) -> bytes:
        # Hash the encoded bytes instead of the decoded data: decoding is slow,
        # it keeps a decoded copy of every stream in memory, and it fails for
        # streams that cannot be decoded.
        return _encodedstreamobject_hash_value_data(self)

    # This overrides the parent method
    def get_data(self) -> bytes:
        return _encodedstreamobject_get_data(self)

    # This overrides the parent method:
    def set_data(self, data: bytes) -> None:
        return _encodedstreamobject_set_data(self, data)


CONTENT_STREAM_ARRAY_MAX_LENGTH = 10_000


class ContentStream(DecodedStreamObject):
    """
    In order to be fast, this data structure can contain either:

    * raw data in ._data
    * parsed stream operations in ._operations.

    At any time, ContentStream object can either have both of those fields defined,
    or one field defined and the other set to None.

    These fields are "rebuilt" lazily, when accessed:

    * when .get_data() is called, if ._data is None, it is rebuilt from ._operations.
    * when .operations is called, if ._operations is None, it is rebuilt from ._data.

    Conversely, these fields can be invalidated:

    * when .set_data() is called, ._operations is set to None.
    * when .operations is set, ._data is set to None.
    """
    _OPERATOR_LENGTH_LIMIT = 128

    def __init__(
        self,
        stream: Any,
        pdf: Any,
        forced_encoding: str | list[str] | dict[int, str] | None = None,
    ) -> None:
        return _contentstream_new(self, stream, pdf, forced_encoding)

    def replicate(
        self,
        pdf_dest: PdfWriterProtocol,
    ) -> "ContentStream":
        return cast("ContentStream", _contentstream_replicate(self, pdf_dest))

    def clone(
        self,
        pdf_dest: Any,
        force_duplicate: bool = False,
        ignore_fields: Sequence[str | int] | None = (),
    ) -> "ContentStream":
        """
        Clone object into pdf_dest.

        Args:
            pdf_dest:
            force_duplicate:
            ignore_fields:

        Returns:
            The cloned ContentStream

        """
        return cast(
            "ContentStream",
            _contentstream_clone(self, pdf_dest, force_duplicate, ignore_fields),
        )

    def _clone(
        self,
        src: DictionaryObject,
        pdf_dest: PdfWriterProtocol,
        force_duplicate: bool,
        ignore_fields: Sequence[str | int] | None,
        visited: set[tuple[int, int]],
    ) -> None:
        """
        Update the object from src.

        Args:
            src:
            pdf_dest:
            force_duplicate:
            ignore_fields:

        """
        return _contentstream_clone_inner(
            self, src, pdf_dest, force_duplicate, ignore_fields, visited
        )

    @staticmethod
    def _check_limits(
        *, remaining_elements: int | None = None, parsing_budget: StreamParsingBudget, token_decrement: int
    ) -> int | None:
        return _contentstream_check_limits(
            remaining_elements=remaining_elements,
            parsing_budget=parsing_budget,
            token_decrement=token_decrement,
        )

    def _parse_content_stream(self, stream: StreamType) -> None:
        # 7.8.2 Content Streams
        return _contentstream_parse_content_stream(self, stream)

    # This overrides the parent method
    def get_data(self) -> bytes:
        return _contentstream_get_data(self)

    # This overrides the parent method
    def set_data(self, data: bytes) -> None:
        return _contentstream_set_data(self, data)

    @property
    def operations(self) -> list[tuple[Any, bytes]]:
        return _contentstream_operations(self)

    @operations.setter
    def operations(self, operations: list[tuple[Any, bytes]]) -> None:
        return _contentstream_set_operations(self, operations)

    def isolate_graphics_state(self) -> None:
        return _contentstream_isolate_graphics_state(self)

    # This overrides the parent method
    def write_to_stream(
        self, stream: StreamType, encryption_key: str | bytes | None = None
    ) -> None:
        return _contentstream_write_to_stream(self, stream, encryption_key)


# Bound comment scanning without consuming Python recursion depth.
_MAX_LEADING_COMMENTS = 1000


def read_object(  # noqa: PLR0911
    stream: StreamType,
    pdf: PdfReaderProtocol | None,
    forced_encoding: str | list[str] | dict[int, str] | None = None,
    *,
    parsing_budget: StreamParsingBudget | None = None,
) -> PdfObject:
    """
    Read the next object from the stream.

    Args:
        stream: The stream to read from.
        pdf: The corresponding PDF file.
        forced_encoding: The encoding to enforce, if any.
        parsing_budget: The allowed parsing budget, optional. If set, only applies to dictionaries,
            arrays and content streams.

    Returns:
        The parsed object.
    """
    return _read_object(stream, pdf, forced_encoding, parsing_budget)


class Field(TreeObject):
    """
    A class representing a field dictionary.

    This class is accessed through
    :meth:`get_fields()<pypdf.PdfReader.get_fields>`
    """

    def __init__(self, data: DictionaryObject) -> None:
        return _field_new(self, data)

    # TABLE 8.69 Entries common to all field dictionaries
    @property
    def field_type(self) -> NameObject | None:
        """Read-only property accessing the type of this field."""
        return _field_field_type(self)

    @property
    def parent(self) -> DictionaryObject | None:
        """Read-only property accessing the parent of this field."""
        return _field_parent(self)

    @property
    def kids(self) -> Optional["ArrayObject"]:
        """Read-only property accessing the kids of this field."""
        return _field_kids(self)

    @property
    def name(self) -> str | None:
        """Read-only property accessing the name of this field."""
        return _field_name(self)

    @property
    def alternate_name(self) -> str | None:
        """Read-only property accessing the alternate name of this field."""
        return _field_alternate_name(self)

    @property
    def mapping_name(self) -> str | None:
        """
        Read-only property accessing the mapping name of this field.

        This name is used by pypdf as a key in the dictionary returned by
        :meth:`get_fields()<pypdf.PdfReader.get_fields>`
        """
        return _field_mapping_name(self)

    @property
    def flags(self) -> int | None:
        """
        Read-only property accessing the field flags, specifying various
        characteristics of the field (see Table 8.70 of the PDF 1.7 reference).
        """
        return _field_flags(self)

    @property
    def value(self) -> Any | None:
        """
        Read-only property accessing the value of this field.

        Format varies based on field type.
        """
        return _field_value(self)

    @property
    def default_value(self) -> Any | None:
        """Read-only property accessing the default value of this field."""
        return _field_default_value(self)

    @property
    def additional_actions(self) -> DictionaryObject | None:
        """
        Read-only property accessing the additional actions dictionary.

        This dictionary defines the field's behavior in response to trigger
        events. See Section 8.5.2 of the PDF 1.7 reference.
        """
        return _field_additional_actions(self)


class Destination(TreeObject):
    """
    A class representing a destination within a PDF file.

    See section 12.3.2 of the PDF 2.0 reference.

    Args:
        title: Title of this destination.
        page: Reference to the page of this destination. Should
            be an instance of :class:`IndirectObject<pypdf.generic.IndirectObject>`.
        fit: How the destination is displayed.

    Raises:
        PdfReadError: If destination type is invalid.

    """

    node: DictionaryObject | None = None  # node provide access to the original Object

    def remove_from_tree(self) -> None:
        """
        Remove the outline item this destination was built from.

        `reader.outline` and `writer.outline` yield detached copies rather than
        the nodes themselves, so the copy never has a `/Parent` and removing it
        would be a no-op. `node` is the dictionary in the document.
        """
        return _destination_remove_from_tree(self)

    def __init__(
        self,
        title: str | bytes,
        page: NumberObject | IndirectObject | NullObject | DictionaryObject,
        fit: Fit,
    ) -> None:
        return _destination_new(self, title, page, fit)

    @property
    def dest_array(self) -> "ArrayObject":
        return _destination_dest_array(self)

    def write_to_stream(self, stream: StreamType) -> None:
        return _destination_write_to_stream(self, stream)

    @property
    def title(self) -> str | None:
        """Read-only property accessing the destination title."""
        return _destination_title(self)

    @property
    def page(self) -> IndirectObject | None:
        """Read-only property accessing the IndirectObject of the destination page."""
        return _destination_page(self)

    @property
    def typ(self) -> str | None:
        """Read-only property accessing the destination type."""
        return _destination_typ(self)

    @property
    def zoom(self) -> int | None:
        """Read-only property accessing the zoom factor."""
        return _destination_zoom(self)

    @property
    def left(self) -> FloatObject | None:
        """Read-only property accessing the left horizontal coordinate."""
        return _destination_left(self)

    @property
    def right(self) -> FloatObject | None:
        """Read-only property accessing the right horizontal coordinate."""
        return _destination_right(self)

    @property
    def top(self) -> FloatObject | None:
        """Read-only property accessing the top vertical coordinate."""
        return _destination_top(self)

    @property
    def bottom(self) -> FloatObject | None:
        """Read-only property accessing the bottom vertical coordinate."""
        return _destination_bottom(self)

    @property
    def color(self) -> Optional["ArrayObject"]:
        """Read-only property accessing the color in (R, G, B) with values 0.0-1.0."""
        return _destination_color(self)

    @property
    def font_format(self) -> OutlineFontFlag | None:
        """
        Read-only property accessing the font type.

        1=italic, 2=bold, 3=both
        """
        return _destination_font_format(self)

    @property
    def outline_count(self) -> int | None:
        """
        Read-only property accessing the outline count.

        positive = expanded
        negative = collapsed
        absolute value = number of visible descendants at all levels
        """
        return _destination_outline_count(self)
