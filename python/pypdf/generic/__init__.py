"""Implementation of generic PDF objects (dictionary, number, string, ...).

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf.generic``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

__author__ = 'Mathieu Fenniak'
__author_email__ = 'biziqe@mathieu.fenniak.net'
from ..constants import OutlineFontFlag
from ._base import BooleanObject, ByteStringObject, FloatObject, IndirectObject, NameObject, NullObject, NumberObject, PdfObject, TextStringObject, encode_pdfdocencoding, is_null_or_none
from ._data_structures import ArrayObject, ContentStream, DecodedStreamObject, Destination, DictionaryObject, EncodedStreamObject, Field, StreamObject, TreeObject, read_object
from ._files import EmbeddedFile
from ._fit import Fit
from ._link import DirectReferenceLink, NamedReferenceLink, ReferenceLink, extract_links
from ._outline import OutlineItem
from ._rectangle import RectangleObject
from ._utils import StreamParsingBudget, create_string_object, decode_pdfdocencoding, hex_to_rgb, read_hex_string_from_stream, read_string_from_stream
from ._viewerpref import ViewerPreferences
PAGE_FIT = None
__all__ = []
