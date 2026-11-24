"""Anything related to Extensible Metadata Platform (XMP) metadata.

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf.xmp``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

import datetime
import decimal
import re
from collections.abc import Callable, Iterator
from string import hexdigits
from typing import Any, NoReturn, TypeVar, cast
from xml.dom.expatbuilder import ExpatBuilderNS
from xml.dom.minidom import Document, ProcessingInstruction
from xml.dom.minidom import Element as XmlElement
from xml.dom.xmlbuilder import Options
from xml.parsers.expat import ExpatError, XMLParserType
from ._configuration import get_configuration
from ._utils import StreamType, deprecation_with_replacement
from .errors import LimitReachedError, PdfReadError, XmpDocumentError
from .generic import ContentStream, PdfObject, StreamObject
XMP_MAX_INPUT_LENGTH = 5000000
XMP_MAX_ELEMENT_COUNT = 100000
RDF_NAMESPACE = 'http://www.w3.org/1999/02/22-rdf-syntax-ns#'
DC_NAMESPACE = 'http://purl.org/dc/elements/1.1/'
XMP_NAMESPACE = 'http://ns.adobe.com/xap/1.0/'
PDF_NAMESPACE = 'http://ns.adobe.com/pdf/1.3/'
XMPMM_NAMESPACE = 'http://ns.adobe.com/xap/1.0/mm/'
PDFX_NAMESPACE = 'http://ns.adobe.com/pdfx/1.3/'
PDFAID_NAMESPACE = 'http://www.aiim.org/pdfa/ns/id/'
_NAMESPACE_PREFIX_MAP = {}
iso8601 = None
K = None
_MINIMAL_XMP = None
def _identity(*args, **kwargs):
    return None  # inert stub: pypdf.xmp._identity
def _converter_date(*args, **kwargs):
    return None  # inert stub: pypdf.xmp._converter_date
def _format_datetime_utc(*args, **kwargs):
    return None  # inert stub: pypdf.xmp._format_datetime_utc
def _generic_get(*args, **kwargs):
    return None  # inert stub: pypdf.xmp._generic_get
class _XmpBuilder(ExpatBuilderNS):
    """Stub for pypdf.xmp._XmpBuilder."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    def custom_entity_declaration_handler(self, *args, **kwargs):
        return None  # inert stub
    def start_element_handler(self, *args, **kwargs):
        return None  # inert stub
    def install(self, *args, **kwargs):
        return None  # inert stub
class XmpInformation(PdfObject):
    """Stub for pypdf.xmp.XmpInformation."""
    def __init__(self, *args, **kwargs):
        try:
            super().__init__(*args, **kwargs)
        except TypeError:
            pass
    @classmethod
    def create(cls, *args, **kwargs):
        return None  # inert stub
    def write_to_stream(self, *args, **kwargs):
        return None  # inert stub
    def get_element(self, *args, **kwargs):
        return None  # inert stub
    def get_nodes_in_namespace(self, *args, **kwargs):
        return None  # inert stub
    def _get_text(self, *args, **kwargs):
        return None  # inert stub
    def _get_single_value(self, *args, **kwargs):
        return None  # inert stub
    def _getter_bag(self, *args, **kwargs):
        return None  # inert stub
    def _get_seq_values(self, *args, **kwargs):
        return None  # inert stub
    def _get_langalt_values(self, *args, **kwargs):
        return None  # inert stub
    @property
    def dc_contributor(self):
        return None  # inert stub
    def dc_contributor(self, *args, **kwargs):
        return None  # inert stub
    @property
    def dc_coverage(self):
        return None  # inert stub
    def dc_coverage(self, *args, **kwargs):
        return None  # inert stub
    @property
    def dc_creator(self):
        return None  # inert stub
    def dc_creator(self, *args, **kwargs):
        return None  # inert stub
    @property
    def dc_date(self):
        return None  # inert stub
    def dc_date(self, *args, **kwargs):
        return None  # inert stub
    @property
    def dc_description(self):
        return None  # inert stub
    def dc_description(self, *args, **kwargs):
        return None  # inert stub
    @property
    def dc_format(self):
        return None  # inert stub
    def dc_format(self, *args, **kwargs):
        return None  # inert stub
    @property
    def dc_identifier(self):
        return None  # inert stub
    def dc_identifier(self, *args, **kwargs):
        return None  # inert stub
    @property
    def dc_language(self):
        return None  # inert stub
    def dc_language(self, *args, **kwargs):
        return None  # inert stub
    @property
    def dc_publisher(self):
        return None  # inert stub
    def dc_publisher(self, *args, **kwargs):
        return None  # inert stub
    @property
    def dc_relation(self):
        return None  # inert stub
    def dc_relation(self, *args, **kwargs):
        return None  # inert stub
    @property
    def dc_rights(self):
        return None  # inert stub
    def dc_rights(self, *args, **kwargs):
        return None  # inert stub
    @property
    def dc_source(self):
        return None  # inert stub
    def dc_source(self, *args, **kwargs):
        return None  # inert stub
    @property
    def dc_subject(self):
        return None  # inert stub
    def dc_subject(self, *args, **kwargs):
        return None  # inert stub
    @property
    def dc_title(self):
        return None  # inert stub
    def dc_title(self, *args, **kwargs):
        return None  # inert stub
    @property
    def dc_type(self):
        return None  # inert stub
    def dc_type(self, *args, **kwargs):
        return None  # inert stub
    @property
    def pdf_keywords(self):
        return None  # inert stub
    def pdf_keywords(self, *args, **kwargs):
        return None  # inert stub
    @property
    def pdf_pdfversion(self):
        return None  # inert stub
    def pdf_pdfversion(self, *args, **kwargs):
        return None  # inert stub
    @property
    def pdf_producer(self):
        return None  # inert stub
    def pdf_producer(self, *args, **kwargs):
        return None  # inert stub
    @property
    def xmp_create_date(self):
        return None  # inert stub
    def xmp_create_date(self, *args, **kwargs):
        return None  # inert stub
    @property
    def xmp_modify_date(self):
        return None  # inert stub
    def xmp_modify_date(self, *args, **kwargs):
        return None  # inert stub
    @property
    def xmp_metadata_date(self):
        return None  # inert stub
    def xmp_metadata_date(self, *args, **kwargs):
        return None  # inert stub
    @property
    def xmp_creator_tool(self):
        return None  # inert stub
    def xmp_creator_tool(self, *args, **kwargs):
        return None  # inert stub
    @property
    def xmpmm_document_id(self):
        return None  # inert stub
    def xmpmm_document_id(self, *args, **kwargs):
        return None  # inert stub
    @property
    def xmpmm_instance_id(self):
        return None  # inert stub
    def xmpmm_instance_id(self, *args, **kwargs):
        return None  # inert stub
    @property
    def pdfaid_part(self):
        return None  # inert stub
    def pdfaid_part(self, *args, **kwargs):
        return None  # inert stub
    @property
    def pdfaid_conformance(self):
        return None  # inert stub
    def pdfaid_conformance(self, *args, **kwargs):
        return None  # inert stub
    @property
    def custom_properties(self):
        return None  # inert stub
    def _get_or_create_description(self, *args, **kwargs):
        return None  # inert stub
    def _clear_cache_entry(self, *args, **kwargs):
        return None  # inert stub
    def _set_single_value(self, *args, **kwargs):
        return None  # inert stub
    def _set_bag_values(self, *args, **kwargs):
        return None  # inert stub
    def _set_seq_values(self, *args, **kwargs):
        return None  # inert stub
    def _set_langalt_values(self, *args, **kwargs):
        return None  # inert stub
    def _get_namespace_prefix(self, *args, **kwargs):
        return None  # inert stub
    def _update_stream(self, *args, **kwargs):
        return None  # inert stub
