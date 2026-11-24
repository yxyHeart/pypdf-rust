"""
Anything related to Extensible Metadata Platform (XMP) metadata.

https://en.wikipedia.org/wiki/Extensible_Metadata_Platform

Rust-backed module: the implementation lives in
``crates/pypdf-py/src/modules/xmp_mod.rs``.  The classes, their MRO, the
``@property`` decorators and the ``ExpatBuilderNS`` subclassing are kept exactly
as upstream, because they are part of the observable Python object model; every
method body delegates to Rust.
"""

import datetime
import decimal
import re
from collections.abc import Callable, Iterator
from string import hexdigits
from typing import (
    Any,
    NoReturn,
    TypeVar,
    cast,
)
from xml.dom.expatbuilder import ExpatBuilderNS
from xml.dom.minidom import Document, ProcessingInstruction
from xml.dom.minidom import Element as XmlElement
from xml.dom.xmlbuilder import Options
from xml.parsers.expat import ExpatError, XMLParserType

from pypdf._pypdf import (
    xmpmod_DC_NAMESPACE as DC_NAMESPACE,
    xmpmod_K as K,
    xmpmod_MINIMAL_XMP as _MINIMAL_XMP,
    xmpmod_NAMESPACE_PREFIX_MAP as _NAMESPACE_PREFIX_MAP,
    xmpmod_PDFAID_NAMESPACE as PDFAID_NAMESPACE,
    xmpmod_PDFX_NAMESPACE as PDFX_NAMESPACE,
    xmpmod_PDF_NAMESPACE as PDF_NAMESPACE,
    xmpmod_RDF_NAMESPACE as RDF_NAMESPACE,
    xmpmod_XMPMM_NAMESPACE as XMPMM_NAMESPACE,
    xmpmod_XMP_MAX_ELEMENT_COUNT as XMP_MAX_ELEMENT_COUNT,
    xmpmod_XMP_MAX_INPUT_LENGTH as XMP_MAX_INPUT_LENGTH,
    xmpmod_XMP_NAMESPACE as XMP_NAMESPACE,
    xmpmod_iso8601 as iso8601,
)

from ._configuration import get_configuration
from ._utils import StreamType, deprecation_with_replacement
from .errors import LimitReachedError, PdfReadError, XmpDocumentError
from .generic import ContentStream, PdfObject, StreamObject

import pypdf._pypdf as _rust

# What is the PDFX namespace, you might ask?
# It's documented here: https://github.com/adobe/xmp-docs/raw/master/XMPSpecifications/XMPSpecificationPart3.pdf
# This namespace is used to place "custom metadata"
# properties, which are arbitrary metadata properties with no semantic or
# documented meaning.
#
# Elements in the namespace are key/value-style storage,
# where the element name is the key and the content is the value. The keys
# are transformed into valid XML identifiers by substituting an invalid
# identifier character with \u2182 followed by the unicode hex ID of the
# original character. A key like "my car" is therefore "my\u21820020car".
#
# \u2182 is the unicode character \u{ROMAN NUMERAL TEN THOUSAND}
#
# The pdfx namespace should be avoided.
# A custom data schema and sensical XML elements could be used instead, as is
# suggested by Adobe's own documentation on XMP under "Extensibility of
# Schemas".


def _identity(value: K) -> K:
    return _rust.xmpmod_identity(value)


def _converter_date(value: str) -> datetime.datetime:
    return _rust.xmpmod_converter_date(value)


def _format_datetime_utc(value: datetime.datetime) -> str:
    """Format a datetime as UTC with trailing 'Z'.

    - If the input is timezone-aware, convert to UTC first.
    - If naive, assume UTC.
    """
    return _rust.xmpmod_format_datetime_utc(value)


def _generic_get(
    element: XmlElement,
    self: "XmpInformation",
    list_type: str,
    converter: Callable[[Any], Any] = _identity,
) -> list[str] | None:
    return _rust.xmpmod_generic_get(element, self, list_type, converter)


class _XmpBuilder(ExpatBuilderNS):
    """
    Custom XML parser denying all entity declarations.

    This is a stripped down and typed version inspired by what *defusedxml* does.

    Why do we need this? The default limits of *libexpat* used by Python only block exponential entity expansion,
    but not cases like quadratic entity expansion which can still cause quite some memory usage.
    """

    def __init__(self, options: Options | None = None) -> None:
        _rust.xmpmod_xmpbuilder_init(self, options)

    def custom_entity_declaration_handler(
        self,
        entity_name: str,
        # expat passes an int here rather than a bool, but typeshed declares
        # the handler with a bool, so the annotation follows typeshed.
        is_parameter_entity: bool,
        value: str | None,
        base: str | None,
        system_id: str,
        public_id: str | None,
        notation_name: str | None,
    ) -> None:
        _rust.xmpmod_xmpbuilder_custom_entity_declaration_handler(
            self,
            entity_name,
            is_parameter_entity,
            value,
            base,
            system_id,
            public_id,
            notation_name,
        )

    def start_element_handler(self, name: str, attributes: list[str]) -> None:
        _rust.xmpmod_xmpbuilder_start_element_handler(self, name, attributes)

    def install(self, parser: XMLParserType) -> None:
        _rust.xmpmod_xmpbuilder_install(self, parser)


class XmpInformation(PdfObject):
    """
    An object that represents Extensible Metadata Platform (XMP) metadata.
    Usually accessed by :py:attr:`xmp_metadata()<pypdf.PdfReader.xmp_metadata>`.

    Raises:
      PdfReadError: if XML is invalid

    """

    def __init__(self, stream: StreamObject) -> None:
        _rust.xmpmod_xmpinformation_init(self, stream)

    @classmethod
    def create(cls) -> "XmpInformation":
        """
        Create a new XmpInformation object with minimal structure.

        Returns:
            A new XmpInformation instance with empty metadata fields.
        """
        return _rust.xmpmod_xmpinformation_create(cls)

    def write_to_stream(self, stream: StreamType) -> NoReturn:
        _rust.xmpmod_xmpinformation_write_to_stream(self, stream)

    def get_element(self, about_uri: str, namespace: str, name: str) -> Iterator[Any]:
        yield from _rust.xmpmod_xmpinformation_get_element_items(
            self, about_uri, namespace, name
        )

    def get_nodes_in_namespace(self, about_uri: str, namespace: str) -> Iterator[Any]:
        yield from _rust.xmpmod_xmpinformation_get_nodes_in_namespace_items(
            self, about_uri, namespace
        )

    def _get_text(self, element: XmlElement) -> str:
        return _rust.xmpmod_xmpinformation_get_text(self, element)

    def _get_single_value(
        self,
        namespace: str,
        name: str,
        converter: Callable[[str], Any] = _identity,
    ) -> Any | None:
        return _rust.xmpmod_xmpinformation_get_single_value(
            self, namespace, name, converter
        )

    def _getter_bag(self, namespace: str, name: str) -> list[str] | None:
        return cast(
            list[str] | None,
            _rust.xmpmod_xmpinformation_getter_bag(self, namespace, name),
        )

    def _get_seq_values(
        self,
        namespace: str,
        name: str,
        converter: Callable[[Any], Any] = _identity,
    ) -> list[Any] | None:
        return cast(
            list[Any] | None,
            _rust.xmpmod_xmpinformation_get_seq_values(
                self, namespace, name, converter
            ),
        )

    def _get_langalt_values(self, namespace: str, name: str) -> dict[Any, Any] | None:
        return cast(
            dict[Any, Any] | None,
            _rust.xmpmod_xmpinformation_get_langalt_values(self, namespace, name),
        )

    @property
    def dc_contributor(self) -> list[str] | None:
        """Contributors to the resource (other than the authors)."""
        return _rust.xmpmod_dc_contributor_get(self)

    @dc_contributor.setter
    def dc_contributor(self, values: list[str] | None) -> None:
        _rust.xmpmod_dc_contributor_set(self, values)

    @property
    def dc_coverage(self) -> str | None:
        """Text describing the extent or scope of the resource."""
        return _rust.xmpmod_dc_coverage_get(self)

    @dc_coverage.setter
    def dc_coverage(self, value: str | None) -> None:
        _rust.xmpmod_dc_coverage_set(self, value)

    @property
    def dc_creator(self) -> list[str] | None:
        """A sorted array of names of the authors of the resource, listed in order of precedence."""
        return _rust.xmpmod_dc_creator_get(self)

    @dc_creator.setter
    def dc_creator(self, values: list[str] | None) -> None:
        _rust.xmpmod_dc_creator_set(self, values)

    @property
    def dc_date(self) -> list[datetime.datetime] | None:
        """A sorted array of dates of significance to the resource. The dates and times are in UTC."""
        return _rust.xmpmod_dc_date_get(self)

    @dc_date.setter
    def dc_date(self, values: list[str | datetime.datetime] | None) -> None:
        _rust.xmpmod_dc_date_set(self, values)

    @property
    def dc_description(self) -> dict[str, str] | None:
        """A language-keyed dictionary of textual descriptions of the content of the resource."""
        return _rust.xmpmod_dc_description_get(self)

    @dc_description.setter
    def dc_description(self, values: dict[str, str] | None) -> None:
        _rust.xmpmod_dc_description_set(self, values)

    @property
    def dc_format(self) -> str | None:
        """The mime-type of the resource."""
        return _rust.xmpmod_dc_format_get(self)

    @dc_format.setter
    def dc_format(self, value: str | None) -> None:
        _rust.xmpmod_dc_format_set(self, value)

    @property
    def dc_identifier(self) -> str | None:
        """Unique identifier of the resource."""
        return _rust.xmpmod_dc_identifier_get(self)

    @dc_identifier.setter
    def dc_identifier(self, value: str | None) -> None:
        _rust.xmpmod_dc_identifier_set(self, value)

    @property
    def dc_language(self) -> list[str] | None:
        """An unordered array specifying the languages used in the resource."""
        return _rust.xmpmod_dc_language_get(self)

    @dc_language.setter
    def dc_language(self, values: list[str] | None) -> None:
        _rust.xmpmod_dc_language_set(self, values)

    @property
    def dc_publisher(self) -> list[str] | None:
        """An unordered array of publisher names."""
        return _rust.xmpmod_dc_publisher_get(self)

    @dc_publisher.setter
    def dc_publisher(self, values: list[str] | None) -> None:
        _rust.xmpmod_dc_publisher_set(self, values)

    @property
    def dc_relation(self) -> list[str] | None:
        """An unordered array of text descriptions of relationships to other documents."""
        return _rust.xmpmod_dc_relation_get(self)

    @dc_relation.setter
    def dc_relation(self, values: list[str] | None) -> None:
        _rust.xmpmod_dc_relation_set(self, values)

    @property
    def dc_rights(self) -> dict[str, str] | None:
        """A language-keyed dictionary of textual descriptions of the rights the user has to this resource."""
        return _rust.xmpmod_dc_rights_get(self)

    @dc_rights.setter
    def dc_rights(self, values: dict[str, str] | None) -> None:
        _rust.xmpmod_dc_rights_set(self, values)

    @property
    def dc_source(self) -> str | None:
        """Unique identifier of the work from which this resource was derived."""
        return _rust.xmpmod_dc_source_get(self)

    @dc_source.setter
    def dc_source(self, value: str | None) -> None:
        _rust.xmpmod_dc_source_set(self, value)

    @property
    def dc_subject(self) -> list[str] | None:
        """An unordered array of descriptive phrases or keywords that specify the topic of the content."""
        return _rust.xmpmod_dc_subject_get(self)

    @dc_subject.setter
    def dc_subject(self, values: list[str] | None) -> None:
        _rust.xmpmod_dc_subject_set(self, values)

    @property
    def dc_title(self) -> dict[str, str] | None:
        """A language-keyed dictionary of the title of the resource."""
        return _rust.xmpmod_dc_title_get(self)

    @dc_title.setter
    def dc_title(self, values: dict[str, str] | None) -> None:
        _rust.xmpmod_dc_title_set(self, values)

    @property
    def dc_type(self) -> list[str] | None:
        """An unordered array of textual descriptions of the document type."""
        return _rust.xmpmod_dc_type_get(self)

    @dc_type.setter
    def dc_type(self, values: list[str] | None) -> None:
        _rust.xmpmod_dc_type_set(self, values)

    @property
    def pdf_keywords(self) -> str | None:
        """An unformatted text string representing document keywords."""
        return _rust.xmpmod_pdf_keywords_get(self)

    @pdf_keywords.setter
    def pdf_keywords(self, value: str | None) -> None:
        _rust.xmpmod_pdf_keywords_set(self, value)

    @property
    def pdf_pdfversion(self) -> str | None:
        """The PDF file version, for example 1.0 or 1.3."""
        return _rust.xmpmod_pdf_pdfversion_get(self)

    @pdf_pdfversion.setter
    def pdf_pdfversion(self, value: str | None) -> None:
        _rust.xmpmod_pdf_pdfversion_set(self, value)

    @property
    def pdf_producer(self) -> str | None:
        """The name of the tool that saved the document as a PDF."""
        return _rust.xmpmod_pdf_producer_get(self)

    @pdf_producer.setter
    def pdf_producer(self, value: str | None) -> None:
        _rust.xmpmod_pdf_producer_set(self, value)

    @property
    def xmp_create_date(self) -> datetime.datetime | None:
        """The date and time the resource was originally created. Returned as a UTC datetime object."""
        return _rust.xmpmod_xmp_create_date_get(self)

    @xmp_create_date.setter
    def xmp_create_date(self, value: datetime.datetime | None) -> None:
        _rust.xmpmod_xmp_create_date_set(self, value)

    @property
    def xmp_modify_date(self) -> datetime.datetime | None:
        """The date and time the resource was last modified. Returned as a UTC datetime object."""
        return _rust.xmpmod_xmp_modify_date_get(self)

    @xmp_modify_date.setter
    def xmp_modify_date(self, value: datetime.datetime | None) -> None:
        _rust.xmpmod_xmp_modify_date_set(self, value)

    @property
    def xmp_metadata_date(self) -> datetime.datetime | None:
        """The date and time that any metadata for this resource was last changed. Returned as a UTC datetime object."""
        return _rust.xmpmod_xmp_metadata_date_get(self)

    @xmp_metadata_date.setter
    def xmp_metadata_date(self, value: datetime.datetime | None) -> None:
        _rust.xmpmod_xmp_metadata_date_set(self, value)

    @property
    def xmp_creator_tool(self) -> str | None:
        """The name of the first known tool used to create the resource."""
        return _rust.xmpmod_xmp_creator_tool_get(self)

    @xmp_creator_tool.setter
    def xmp_creator_tool(self, value: str | None) -> None:
        _rust.xmpmod_xmp_creator_tool_set(self, value)

    @property
    def xmpmm_document_id(self) -> str | None:
        """The common identifier for all versions and renditions of this resource."""
        return _rust.xmpmod_xmpmm_document_id_get(self)

    @xmpmm_document_id.setter
    def xmpmm_document_id(self, value: str | None) -> None:
        _rust.xmpmod_xmpmm_document_id_set(self, value)

    @property
    def xmpmm_instance_id(self) -> str | None:
        """An identifier for a specific incarnation of a document, updated each time a file is saved."""
        return _rust.xmpmod_xmpmm_instance_id_get(self)

    @xmpmm_instance_id.setter
    def xmpmm_instance_id(self, value: str | None) -> None:
        _rust.xmpmod_xmpmm_instance_id_set(self, value)

    @property
    def pdfaid_part(self) -> str | None:
        """The part of the PDF/A standard that the document conforms to (e.g., 1, 2, 3)."""
        return _rust.xmpmod_pdfaid_part_get(self)

    @pdfaid_part.setter
    def pdfaid_part(self, value: str | None) -> None:
        _rust.xmpmod_pdfaid_part_set(self, value)

    @property
    def pdfaid_conformance(self) -> str | None:
        """The conformance level within the PDF/A standard (e.g., 'A', 'B', 'U')."""
        return _rust.xmpmod_pdfaid_conformance_get(self)

    @pdfaid_conformance.setter
    def pdfaid_conformance(self, value: str | None) -> None:
        _rust.xmpmod_pdfaid_conformance_set(self, value)

    @property
    def custom_properties(self) -> dict[Any, Any]:
        """
        Retrieve custom metadata properties defined in the undocumented pdfx
        metadata schema.

        Returns:
            A dictionary of key/value items for custom metadata properties.

        """
        return _rust.xmpmod_xmpinformation_custom_properties(self)

    def _get_or_create_description(self, about_uri: str = "") -> XmlElement:
        """Get or create an rdf:Description element with the given about URI."""
        return _rust.xmpmod_xmpinformation_get_or_create_description(self, about_uri)

    def _clear_cache_entry(self, namespace: str, name: str) -> None:
        """Remove a cached value for a given namespace/name if present."""
        _rust.xmpmod_xmpinformation_clear_cache_entry(self, namespace, name)

    def _set_single_value(self, namespace: str, name: str, value: str | None) -> None:
        """Set or remove a single metadata value."""
        _rust.xmpmod_xmpinformation_set_single_value(self, namespace, name, value)

    def _set_bag_values(self, namespace: str, name: str, values: list[str] | None) -> None:
        """Set or remove bag values (unordered array)."""
        _rust.xmpmod_xmpinformation_set_bag_values(self, namespace, name, values)

    def _set_seq_values(self, namespace: str, name: str, values: list[str] | None) -> None:
        """Set or remove sequence values (ordered array)."""
        _rust.xmpmod_xmpinformation_set_seq_values(self, namespace, name, values)

    def _set_langalt_values(
        self, namespace: str, name: str, values: dict[str, str] | None
    ) -> None:
        """Set or remove language alternative values."""
        _rust.xmpmod_xmpinformation_set_langalt_values(self, namespace, name, values)

    def _get_namespace_prefix(self, namespace: str) -> str:
        """Get the appropriate namespace prefix for a given namespace URI."""
        return _rust.xmpmod_xmpinformation_get_namespace_prefix(self, namespace)

    def _update_stream(self) -> None:
        """Update the stream with the current XML content."""
        _rust.xmpmod_xmpinformation_update_stream(self)

# --- upstream type-annotation parity (validation §4.2) -----------------------
from pypdf._api_annotations import apply_module as _apply_api_annotations

_apply_api_annotations(globals())
globals().pop("annotations", None)  # `from __future__ import annotations` binds this name; upstream modules do not have it
