"""Various constants, enums, and flags to aid readability.

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf.constants``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

import sys
from enum import Enum, IntFlag, auto, unique
from typing import Any
from ._utils import deprecation_with_replacement
if sys.version_info >= (3, 11):
    from enum import StrEnum
else:
    class StrEnum:
        """Stub for pypdf.constants.StrEnum."""
        def __str__(self, *args, **kwargs):
            return ""  # inert stub
class Core:
    """Stub for pypdf.constants.Core."""
    OUTLINES = '/Outlines'
    THREADS = '/Threads'
    PAGE = '/Page'
    PAGES = '/Pages'
    CATALOG = '/Catalog'
class TrailerKeys:
    """Stub for pypdf.constants.TrailerKeys."""
    SIZE = '/Size'
    PREV = '/Prev'
    ROOT = '/Root'
    ENCRYPT = '/Encrypt'
    INFO = '/Info'
    ID = '/ID'
class CatalogAttributes:
    """Stub for pypdf.constants.CatalogAttributes."""
    TYPE = '/Type'
    VERSION = '/Version'
    EXTENSIONS = '/Extensions'
    PAGES = '/Pages'
    PAGE_LABELS = '/PageLabels'
    NAMES = '/Names'
    DESTS = '/Dests'
    VIEWER_PREFERENCES = '/ViewerPreferences'
    PAGE_LAYOUT = '/PageLayout'
    PAGE_MODE = '/PageMode'
    OUTLINES = '/Outlines'
    THREADS = '/Threads'
    OPEN_ACTION = '/OpenAction'
    AA = '/AA'
    URI = '/URI'
    ACRO_FORM = '/AcroForm'
    METADATA = '/Metadata'
    STRUCT_TREE_ROOT = '/StructTreeRoot'
    MARK_INFO = '/MarkInfo'
    LANG = '/Lang'
    SPIDER_INFO = '/SpiderInfo'
    OUTPUT_INTENTS = '/OutputIntents'
    PIECE_INFO = '/PieceInfo'
    OC_PROPERTIES = '/OCProperties'
    PERMS = '/Perms'
    LEGAL = '/Legal'
    REQUIREMENTS = '/Requirements'
    COLLECTION = '/Collection'
    NEEDS_RENDERING = '/NeedsRendering'
    DSS = '/DSS'
    AF = '/AF'
    D_PART_ROOT = '/DPartRoot'
class EncryptionDictAttributes:
    """Stub for pypdf.constants.EncryptionDictAttributes."""
    R = '/R'
    O = '/O'
    U = '/U'
    P = '/P'
    ENCRYPT_METADATA = '/EncryptMetadata'
class UserAccessPermissions:
    """Stub for pypdf.constants.UserAccessPermissions."""
    R1 = 1
    R2 = 2
    PRINT = 4
    MODIFY = 8
    EXTRACT = 16
    ADD_OR_MODIFY = 32
    R7 = 64
    R8 = 128
    FILL_FORM_FIELDS = 256
    EXTRACT_TEXT_AND_GRAPHICS = 512
    ASSEMBLE_DOC = 1024
    PRINT_TO_REPRESENTATION = 2048
    R13 = None
    R14 = None
    R15 = None
    R16 = None
    R17 = None
    R18 = None
    R19 = None
    R20 = None
    R21 = None
    R22 = None
    R23 = None
    R24 = None
    R25 = None
    R26 = None
    R27 = None
    R28 = None
    R29 = None
    R30 = None
    R31 = None
    R32 = None
    @classmethod
    def _is_reserved(cls, *args, **kwargs):
        return None  # inert stub
    @classmethod
    def _is_active(cls, *args, **kwargs):
        return None  # inert stub
    def to_dict(self, *args, **kwargs):
        return None  # inert stub
    @classmethod
    def from_dict(cls, *args, **kwargs):
        return None  # inert stub
    @classmethod
    def all(cls, *args, **kwargs):
        return None  # inert stub
class Resources:
    """Stub for pypdf.constants.Resources."""
    EXT_G_STATE = '/ExtGState'
    COLOR_SPACE = '/ColorSpace'
    PATTERN = '/Pattern'
    SHADING = '/Shading'
    XOBJECT = '/XObject'
    FONT = '/Font'
    PROC_SET = '/ProcSet'
    PROPERTIES = '/Properties'
class PagesAttributes:
    """Stub for pypdf.constants.PagesAttributes."""
    TYPE = '/Type'
    PARENT = '/Parent'
    KIDS = '/Kids'
    COUNT = '/Count'
class PageAttributes:
    """Stub for pypdf.constants.PageAttributes."""
    TYPE = '/Type'
    PARENT = '/Parent'
    LAST_MODIFIED = '/LastModified'
    RESOURCES = '/Resources'
    MEDIABOX = '/MediaBox'
    CROPBOX = '/CropBox'
    BLEEDBOX = '/BleedBox'
    TRIMBOX = '/TrimBox'
    ARTBOX = '/ArtBox'
    BOX_COLOR_INFO = '/BoxColorInfo'
    CONTENTS = '/Contents'
    ROTATE = '/Rotate'
    GROUP = '/Group'
    THUMB = '/Thumb'
    B = '/B'
    DUR = '/Dur'
    TRANS = '/Trans'
    ANNOTS = '/Annots'
    AA = '/AA'
    METADATA = '/Metadata'
    PIECE_INFO = '/PieceInfo'
    STRUCT_PARENTS = '/StructParents'
    ID = '/ID'
    PZ = '/PZ'
    SEPARATION_INFO = '/SeparationInfo'
    TABS = '/Tabs'
    TEMPLATE_INSTANTIATED = '/TemplateInstantiated'
    PRES_STEPS = '/PresSteps'
    USER_UNIT = '/UserUnit'
    VP = '/VP'
    AF = '/AF'
    OUTPUT_INTENTS = '/OutputIntents'
    D_PART = '/DPart'
class FileSpecificationDictionaryEntries:
    """Stub for pypdf.constants.FileSpecificationDictionaryEntries."""
    Type = '/Type'
    FS = '/FS'
    F = '/F'
    UF = '/UF'
    DOS = '/DOS'
    Mac = '/Mac'
    Unix = '/Unix'
    ID = '/ID'
    V = '/V'
    EF = '/EF'
    RF = '/RF'
    DESC = '/Desc'
    Cl = '/Cl'
class StreamAttributes:
    """Stub for pypdf.constants.StreamAttributes."""
    LENGTH = '/Length'
    FILTER = '/Filter'
    DECODE_PARMS = '/DecodeParms'
class FilterTypes:
    """Stub for pypdf.constants.FilterTypes."""
    ASCII_HEX_DECODE = '/ASCIIHexDecode'
    ASCII_85_DECODE = '/ASCII85Decode'
    LZW_DECODE = '/LZWDecode'
    FLATE_DECODE = '/FlateDecode'
    RUN_LENGTH_DECODE = '/RunLengthDecode'
    CCITT_FAX_DECODE = '/CCITTFaxDecode'
    DCT_DECODE = '/DCTDecode'
    JPX_DECODE = '/JPXDecode'
    BROTLI_DECODE = '/BrotliDecode'
    JBIG2_DECODE = '/JBIG2Decode'
class FilterTypeAbbreviations:
    """Stub for pypdf.constants.FilterTypeAbbreviations."""
    AHx = '/AHx'
    A85 = '/A85'
    LZW = '/LZW'
    FL = '/Fl'
    RL = '/RL'
    CCF = '/CCF'
    DCT = '/DCT'
class LzwFilterParameters:
    """Stub for pypdf.constants.LzwFilterParameters."""
    PREDICTOR = '/Predictor'
    COLORS = '/Colors'
    BITS_PER_COMPONENT = '/BitsPerComponent'
    COLUMNS = '/Columns'
    EARLY_CHANGE = '/EarlyChange'
class CcittFaxDecodeParameters:
    """Stub for pypdf.constants.CcittFaxDecodeParameters."""
    K = '/K'
    END_OF_LINE = '/EndOfLine'
    ENCODED_BYTE_ALIGN = '/EncodedByteAlign'
    COLUMNS = '/Columns'
    ROWS = '/Rows'
    END_OF_BLOCK = '/EndOfBlock'
    BLACK_IS_1 = '/BlackIs1'
    DAMAGED_ROWS_BEFORE_ERROR = '/DamagedRowsBeforeError'
class ImageAttributes:
    """Stub for pypdf.constants.ImageAttributes."""
    TYPE = '/Type'
    SUBTYPE = '/Subtype'
    NAME = '/Name'
    WIDTH = '/Width'
    HEIGHT = '/Height'
    BITS_PER_COMPONENT = '/BitsPerComponent'
    COLOR_SPACE = '/ColorSpace'
    DECODE = '/Decode'
    INTENT = '/Intent'
    INTERPOLATE = '/Interpolate'
    IMAGE_MASK = '/ImageMask'
    MASK = '/Mask'
    S_MASK = '/SMask'
class ColorSpaces:
    """Stub for pypdf.constants.ColorSpaces."""
    DEVICE_RGB = '/DeviceRGB'
    DEVICE_CMYK = '/DeviceCMYK'
    DEVICE_GRAY = '/DeviceGray'
class TypArguments:
    """Stub for pypdf.constants.TypArguments."""
    LEFT = '/Left'
    RIGHT = '/Right'
    BOTTOM = '/Bottom'
    TOP = '/Top'
class TypFitArguments:
    """Stub for pypdf.constants.TypFitArguments."""
    XYZ = '/XYZ'
    FIT = '/Fit'
    FIT_H = '/FitH'
    FIT_V = '/FitV'
    FIT_R = '/FitR'
    FIT_B = '/FitB'
    FIT_BH = '/FitBH'
    FIT_BV = '/FitBV'
class GoToActionArguments:
    """Stub for pypdf.constants.GoToActionArguments."""
    S = '/S'
    D = '/D'
    SD = '/SD'
class AnnotationDictionaryAttributes:
    """Stub for pypdf.constants.AnnotationDictionaryAttributes."""
    Type = '/Type'
    Subtype = '/Subtype'
    Rect = '/Rect'
    Contents = '/Contents'
    P = '/P'
    NM = '/NM'
    M = '/M'
    F = '/F'
    AP = '/AP'
    AS = '/AS'
    DA = '/DA'
    Border = '/Border'
    C = '/C'
    StructParent = '/StructParent'
    OC = '/OC'
class InteractiveFormDictEntries:
    """Stub for pypdf.constants.InteractiveFormDictEntries."""
    Fields = '/Fields'
    NeedAppearances = '/NeedAppearances'
    SigFlags = '/SigFlags'
    CO = '/CO'
    DR = '/DR'
    DA = '/DA'
    Q = '/Q'
    XFA = '/XFA'
class FieldDictionaryAttributes:
    """Stub for pypdf.constants.FieldDictionaryAttributes."""
    FT = '/FT'
    Parent = '/Parent'
    Kids = '/Kids'
    T = '/T'
    TU = '/TU'
    TM = '/TM'
    Ff = '/Ff'
    V = '/V'
    DV = '/DV'
    AA = '/AA'
    Opt = '/Opt'
    class FfBits:
        """Stub for pypdf.constants.FfBits."""
        ReadOnly = None
        Required = None
        NoExport = None
        Multiline = None
        Password = None
        NoToggleToOff = None
        Radio = None
        Pushbutton = None
        Combo = None
        Edit = None
        Sort = None
        FileSelect = None
        MultiSelect = None
        DoNotSpellCheck = None
        DoNotScroll = None
        Comb = None
        RadiosInUnison = None
        RichText = None
        CommitOnSelChange = None
    @classmethod
    def attributes(cls, *args, **kwargs):
        return None  # inert stub
    @classmethod
    def attributes_dict(cls, *args, **kwargs):
        return None  # inert stub
class CheckboxRadioButtonAttributes:
    """Stub for pypdf.constants.CheckboxRadioButtonAttributes."""
    Opt = '/Opt'
    @classmethod
    def attributes(cls, *args, **kwargs):
        return None  # inert stub
    @classmethod
    def attributes_dict(cls, *args, **kwargs):
        return None  # inert stub
class FieldFlag:
    """Stub for pypdf.constants.FieldFlag."""
    READ_ONLY = 1
    REQUIRED = 2
    NO_EXPORT = 4
class DocumentInformationAttributes:
    """Stub for pypdf.constants.DocumentInformationAttributes."""
    TITLE = '/Title'
    AUTHOR = '/Author'
    SUBJECT = '/Subject'
    KEYWORDS = '/Keywords'
    CREATOR = '/Creator'
    PRODUCER = '/Producer'
    CREATION_DATE = '/CreationDate'
    MOD_DATE = '/ModDate'
    TRAPPED = '/Trapped'
class PageLayouts:
    """Stub for pypdf.constants.PageLayouts."""
    SINGLE_PAGE = '/SinglePage'
    ONE_COLUMN = '/OneColumn'
    TWO_COLUMN_LEFT = '/TwoColumnLeft'
    TWO_COLUMN_RIGHT = '/TwoColumnRight'
    TWO_PAGE_LEFT = '/TwoPageLeft'
    TWO_PAGE_RIGHT = '/TwoPageRight'
class GraphicsStateParameters:
    """Stub for pypdf.constants.GraphicsStateParameters."""
    TYPE = '/Type'
    LW = '/LW'
    LC = '/LC'
    LJ = '/LJ'
    ML = '/ML'
    D = '/D'
    RI = '/RI'
    OP = '/OP'
    op = '/op'
    OPM = '/OPM'
    FONT = '/Font'
    BG = '/BG'
    BG2 = '/BG2'
    UCR = '/UCR'
    UCR2 = '/UCR2'
    TR = '/TR'
    TR2 = '/TR2'
    HT = '/HT'
    FL = '/FL'
    SM = '/SM'
    SA = '/SA'
    BM = '/BM'
    S_MASK = '/SMask'
    CA = '/CA'
    ca = '/ca'
    AIS = '/AIS'
    TK = '/TK'
class _CatalogDictionaryMeta(type):
    """Stub for pypdf.constants._CatalogDictionaryMeta."""
    def __getattribute__(self, *args, **kwargs):
        return None  # inert stub
class CatalogDictionary(CatalogAttributes):
    """Stub for pypdf.constants.CatalogDictionary."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
class OutlineFontFlag:
    """Stub for pypdf.constants.OutlineFontFlag."""
    italic = 1
    bold = 2
class PageLabelStyle:
    """Stub for pypdf.constants.PageLabelStyle."""
    DECIMAL = '/D'
    UPPERCASE_ROMAN = '/R'
    LOWERCASE_ROMAN = '/r'
    UPPERCASE_LETTER = '/A'
    LOWERCASE_LETTER = '/a'
class AnnotationFlag:
    """Stub for pypdf.constants.AnnotationFlag."""
    INVISIBLE = 1
    HIDDEN = 2
    PRINT = 4
    NO_ZOOM = 8
    NO_ROTATE = 16
    NO_VIEW = 32
    READ_ONLY = 64
    LOCKED = 128
    TOGGLE_NO_VIEW = 256
    LOCKED_CONTENTS = 512
PDF_KEYS = ()
class ImageType:
    """Stub for pypdf.constants.ImageType."""
    NONE = 0
    XOBJECT_IMAGES = None
    INLINE_IMAGES = None
    DRAWING_IMAGES = None
    ALL = None
    IMAGES = None
_INLINE_IMAGE_VALUE_MAPPING = {}
_INLINE_IMAGE_KEY_MAPPING = {}
class AFRelationship:
    """Stub for pypdf.constants.AFRelationship."""
    SOURCE = '/Source'
    DATA = '/Data'
    ALTERNATIVE = '/Alternative'
    SUPPLEMENT = '/Supplement'
    ENCRYPTED_PAYLOAD = '/EncryptedPayload'
    FORM_DATA = '/FormData'
    SCHEMA = '/Schema'
    UNSPECIFIED = '/Unspecified'
class BorderStyles:
    """Stub for pypdf.constants.BorderStyles."""
    BEVELED = '/B'
    DASHED = '/D'
    INSET = '/I'
    SOLID = '/S'
    UNDERLINED = '/U'
class FontFlags:
    """Stub for pypdf.constants.FontFlags."""
    FIXED_PITCH = None
    SERIF = None
    SYMBOLIC = None
    SCRIPT = None
    NONSYMBOLIC = None
    ITALIC = None
    ALL_CAP = None
    SMALL_CAP = None
    FORCE_BOLD = None
