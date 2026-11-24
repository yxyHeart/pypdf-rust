"""pypdf is a free and open-source pure-python PDF library capable of splitting,

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

from ._configuration import Configuration, apply_configuration, get_configuration, overwrite_configuration
from ._crypt_providers import crypt_provider
from ._doc_common import DocumentInformation
from ._encryption import PasswordType
from ._page import PageObject, Transformation
from ._reader import PdfReader
from ._text_extraction import mult
from ._version import __version__
from ._writer import ObjectDeletionFlag, PdfWriter
from .constants import ImageType
from .pagerange import PageRange, parse_filename_page_ranges
from .papersizes import PaperSize
try:
    import PIL
    pil_version = None
except ImportError:
    pil_version = 'none'
_debug_versions = None
__all__ = []
