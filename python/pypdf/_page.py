# Copyright (c) 2006, Mathieu Fenniak
# Copyright (c) 2007, Ashish Kulkarni <kulkarni.ashish@gmail.com>
#
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

"""Rust-backed port of ``pypdf/_page.py`` (upstream pypdf 7.1.1).

The class hierarchy, class attributes, properties, decorators and method
signatures are kept verbatim from upstream so that the observable Python object
model (MRO, ``isinstance``, subclassing, ``sys.settrace`` call graphs) is
unchanged.  Every method body delegates to the Rust implementation in
``crates/pypdf-py/src/modules/page_mod.rs`` (reached through the
``pypdf._pypdf`` extension module), which contains all of the actual logic.
"""

import math
from collections.abc import Callable, Iterable, Iterator, Sequence
from copy import deepcopy
from dataclasses import asdict, dataclass
from decimal import Decimal
from io import BytesIO
from pathlib import Path
from typing import (
    Any,
    Literal,
    NoReturn,
    cast,
    overload,
)

from pypdf._pypdf import (
    page_add_transformation_matrix as _add_transformation_matrix_impl,
    page_content_stream_rename as _content_stream_rename_impl,
    page_create_blank_page as _create_blank_page_impl,
    page_delete_rectangle as _delete_rectangle_impl,
    page_delete_action as _delete_action_impl,
    page_expand_mediabox as _expand_mediabox_impl,
    page_extract_text as _extract_text_public_impl,
    page_extract_text_inner as _extract_text_impl,
    page_extract_text_xform as _extract_text_xform_impl,
    page_extract_xform_text as _extract_xform_text_impl,
    page_get_contents as _get_contents_impl,
    page_get_contents_as_bytes as _get_contents_as_bytes_impl,
    page_get_font_resources as _get_font_resources_impl,
    page_get_fonts as _get_fonts_impl,
    page_get_fonts_walk as _get_fonts_walk_impl,
    page_get_ids_image as _get_ids_image_impl,
    page_get_image as _get_image_impl,
    page_get_page_resources as _get_page_resources_impl,
    page_get_rectangle as _get_rectangle_impl,
    page_hash_bin as _hash_bin_impl,
    page_hash_value_data as _hash_value_data_impl,
    page_imagefile_repr as _imagefile_repr,
    page_imagefile_replace as _imagefile_replace,
    page_imagefile_str as _imagefile_str,
    page_inline_images_get as _inline_images_get,
    page_inline_images_set as _inline_images_set,
    page_layout_mode_fonts as _layout_mode_fonts_impl,
    page_layout_mode_text as _layout_mode_text_impl,
    page_merge_page as _merge_page_impl,
    page_merge_page_inner as _merge_page_inner_impl,
    page_merge_page_writer as _merge_page_writer_impl,
    page_merge_resources as _merge_resources_impl,
    page_number as _page_number_impl,
    page_parse_images_from_content_stream as _parse_images_from_content_stream_impl,
    page_replace_contents as _replace_contents_impl,
    page_rotate as _rotate_impl,
    page_rotation_get as _rotation_get,
    page_rotation_set as _rotation_set,
    page_scale as _scale_impl,
    page_scale_by as _scale_by_impl,
    page_scale_to as _scale_to_impl,
    page_set_rectangle as _set_rectangle_impl,
    page_transfer_rotation_to_content as _transfer_rotation_to_content_impl,
    page_transformation_apply_on as _transformation_apply_on,
    page_transformation_compress as _transformation_compress,
    page_transformation_matrix as _transformation_matrix,
    page_transformation_repr as _transformation_repr,
    page_transformation_rotate as _transformation_rotate,
    page_transformation_scale as _transformation_scale,
    page_transformation_to_cm as _transformation_to_cm,
    page_transformation_transform as _transformation_transform,
    page_transformation_translate as _transformation_translate,
    page_translate_value_inline_image as _translate_value_inline_image_impl,
    page_virtual_list_delitem as _virtual_list_delitem,
    page_virtual_list_getitem as _virtual_list_getitem,
    page_virtual_list_len as _virtual_list_len,
    page_virtual_list_str as _virtual_list_str,
    page_virtual_listimages_getitem as _virtual_listimages_getitem,
    page_virtual_listimages_items as _virtual_listimages_items,
    page_virtual_listimages_keys as _virtual_listimages_keys,
    page_virtual_listimages_len as _virtual_listimages_len,
    page_virtual_listimages_str as _virtual_listimages_str,
    page_add_action as _add_action_impl,
    page_compress_content_streams as _compress_content_streams_impl,
    page_debug_for_extract as _debug_for_extract_impl,
    page_images as _images_impl,
    page_init as _page_init_impl,
    page_user_unit as _user_unit_impl,
    page_add_transformation as _page_add_transformation_impl,
    page_merge_scaled_page as _merge_scaled_page_impl,
    page_merge_rotated_page as _merge_rotated_page_impl,
    page_merge_translated_page as _merge_translated_page_impl,
    page_merge_transformed_page as _merge_transformed_page_impl,
    page_annotations_get as _annotations_get_impl,
    page_annotations_set as _annotations_set_impl,
)

from ._configuration import get_configuration
from ._protocols import PdfCommonDocProtocol
from ._text_extraction import (
    _layout_mode,
)
from ._text_extraction._text_extractor import TextExtraction
from ._utils import (
    CompressedTransformationMatrix,
    TransformationMatrixType,
    _human_readable_bytes,
    _TraversalState,
    deprecation,
    deprecation_no_replacement,
    deprecation_with_replacement,
    logger_warning,
    matrix_multiply,
)
from .actions import Action, PageTrigger
from .constants import (
    _INLINE_IMAGE_KEY_MAPPING,
    _INLINE_IMAGE_VALUE_MAPPING,
    AnnotationDictionaryAttributes,
    ImageAttributes,
    PageAttributes,
    Resources,
)
from .errors import PageSizeNotDefinedError, PdfReadError
from .generic import (
    ArrayObject,
    ContentStream,
    DictionaryObject,
    EncodedStreamObject,
    FloatObject,
    IndirectObject,
    NameObject,
    NullObject,
    NumberObject,
    PdfObject,
    RectangleObject,
    StreamObject,
    is_null_or_none,
)
from .generic._font import Font

try:
    from PIL.Image import Image

    pil_not_imported = False
except ImportError:
    Image = object  # type: ignore[assignment,misc,unused-ignore]  # TODO: Remove unused-ignore on Python 3.10
    pil_not_imported = True  # error will be raised only when using images

MERGE_CROP_BOX = "cropbox"  # DEPRECATED: Use pypdf.Configuration.


def _get_page_resources(obj: Any, inherit: bool = True) -> DictionaryObject:
    """
    Return the /Resources, or an empty dictionary if missing or malformed.

    With ``inherit=False``, only the /Resources set on the page itself are used,
    not those inherited from its parents.
    """
    return _get_page_resources_impl(obj, inherit)


def _get_font_resources(resources: Any) -> DictionaryObject:
    """Return the /Font resources, or an empty dictionary if missing or malformed."""
    return _get_font_resources_impl(resources)


def _get_rectangle(self: Any, name: str, defaults: Iterable[str]) -> RectangleObject:
    return _get_rectangle_impl(self, name, defaults)


def _set_rectangle(self: Any, name: str, value: RectangleObject | float) -> None:
    return _set_rectangle_impl(self, name, value)


def _delete_rectangle(self: Any, name: str) -> None:
    return _delete_rectangle_impl(self, name)


def _create_rectangle_accessor(name: str, fallback: Iterable[str]) -> property:
    return property(
        lambda self: _get_rectangle(self, name, fallback),
        lambda self, value: _set_rectangle(self, name, value),
        lambda self: _delete_rectangle(self, name),
    )


class Transformation:
    """
    Represent a 2D transformation.

    The transformation between two coordinate systems is represented by a 3-by-3
    transformation matrix with the following form::

        a b 0
        c d 0
        e f 1

    Because a transformation matrix has only six elements that can be changed,
    it is usually specified in PDF as the six-element array [ a b c d e f ].

    Coordinate transformations are expressed as matrix multiplications::

                                 a b 0
     [ x′ y′ 1 ] = [ x y 1 ] ×   c d 0
                                 e f 1


    Example:
        >>> from pypdf import PdfWriter, Transformation
        >>> page = PdfWriter().add_blank_page(800, 600)
        >>> op = Transformation().scale(sx=2, sy=3).translate(tx=10, ty=20)
        >>> page.add_transformation(op)

    """

    def __init__(self, ctm: CompressedTransformationMatrix = (1, 0, 0, 1, 0, 0)) -> None:
        self.ctm = ctm

    @property
    def matrix(self) -> TransformationMatrixType:
        """
        Return the transformation matrix as a tuple of tuples in the form:

        ((a, b, 0), (c, d, 0), (e, f, 1))
        """
        return _transformation_matrix(self)

    @staticmethod
    def compress(matrix: TransformationMatrixType) -> CompressedTransformationMatrix:
        """
        Compresses the transformation matrix into a tuple of (a, b, c, d, e, f).

        Args:
            matrix: The transformation matrix as a tuple of tuples.

        Returns:
            A tuple representing the transformation matrix as (a, b, c, d, e, f)

        """
        return _transformation_compress(matrix)

    def _to_cm(self) -> str:
        # Returns the cm operation string for the given transformation matrix
        return _transformation_to_cm(self)

    def transform(self, m: "Transformation") -> "Transformation":
        """
        Apply one transformation to another.

        Args:
            m: a Transformation to apply.

        Returns:
            A new ``Transformation`` instance

        Example:
            >>> from pypdf import PdfWriter, Transformation
            >>> height, width = 40, 50
            >>> page = PdfWriter().add_blank_page(800, 600)
            >>> op = Transformation((1, 0, 0, -1, 0, height)) # vertical mirror
            >>> op = Transformation().transform(Transformation((-1, 0, 0, 1, width, 0)))  # horizontal mirror
            >>> page.add_transformation(op)

        """
        return _transformation_transform(self, m)

    def translate(self, tx: float = 0, ty: float = 0) -> "Transformation":
        """
        Translate the contents of a page.

        Args:
            tx: The translation along the x-axis.
            ty: The translation along the y-axis.

        Returns:
            A new ``Transformation`` instance

        """
        return _transformation_translate(self, tx, ty)

    def scale(
        self, sx: float | None = None, sy: float | None = None
    ) -> "Transformation":
        """
        Scale the contents of a page towards the origin of the coordinate system.

        Typically, that is the lower-left corner of the page. That can be
        changed by translating the contents / the page boxes.

        Args:
            sx: The scale factor along the x-axis.
            sy: The scale factor along the y-axis.

        Returns:
            A new Transformation instance with the scaled matrix.

        """
        return _transformation_scale(self, sx, sy)

    def rotate(self, rotation: float) -> "Transformation":
        """
        Rotate the contents of a page.

        Args:
            rotation: The angle of rotation in degrees.

        Returns:
            A new ``Transformation`` instance with the rotated matrix.

        """
        return _transformation_rotate(self, rotation)

    def __repr__(self) -> str:
        return _transformation_repr(self)

    def apply_on(
        self,
        pt: tuple[float, float] | list[float],
        as_object: bool = False,
    ) -> tuple[float, float] | list[float]:
        """
        Apply the transformation matrix on the given point.

        Args:
            pt: A tuple or list representing the point in the form (x, y).
            as_object: If True, return items as FloatObject, otherwise as plain floats.

        Returns:
            A tuple or list representing the transformed point in the form (x', y')

        """
        return _transformation_apply_on(self, pt, as_object)


@dataclass
class ImageFile:
    """
    Image within the PDF file. *This object is not designed to be built.*

    This object should not be modified except using :func:`ImageFile.replace` to replace the image with a new one.
    """

    name: str = ""
    """
    Filename as identified within the PDF file.

    .. warning::

        This value can contain arbitrary characters. Please make sure to sanitize it before
        using it to write the file content to the disk for example.
    """

    data: bytes = b""
    """
    Data as bytes.
    """

    image: Image | None = None
    """
    Data as PIL image.
    """

    indirect_reference: IndirectObject | None = None
    """
    Reference to the object storing the stream.
    """

    is_inline: bool = False
    """
    True if this is an inline image (~0~, ~1~, etc.).
    """

    is_displayed: bool = False
    """
    True if this image is displayed in the page content stream.
    """

    def replace(self, new_image: Image, **kwargs: Any) -> None:
        """
        Replace the image with a new PIL image.

        Args:
            new_image (PIL.Image.Image): The new PIL image to replace the existing image.
            **kwargs: Additional keyword arguments to pass to `Image.save()`.

        Raises:
            TypeError: If the image is inline or in a PdfReader.
            TypeError: If the image does not belong to a PdfWriter.
            TypeError: If `new_image` is not a PIL Image.

        Note:
            This method replaces the existing image with a new image.
            It is not allowed for inline images or images within a PdfReader.
            The `kwargs` parameter allows passing additional parameters
            to `Image.save()`, such as quality.

        """
        return _imagefile_replace(self, new_image, kwargs)

    def __str__(self) -> str:
        return _imagefile_str(self)

    def __repr__(self) -> str:
        return _imagefile_repr(self)


class VirtualListImages(Sequence[ImageFile]):
    """
    Provides access to images referenced within a page.
    Only one copy will be returned if the usage is used on the same page multiple times.
    See :func:`PageObject.images` for more details.
    """

    def __init__(
        self,
        ids_function: Callable[[], list[str | list[str]]],
        get_function: Callable[[str | list[str] | tuple[str]], ImageFile],
    ) -> None:
        self.ids_function = ids_function
        self.get_function = get_function
        self.current = -1

    def __len__(self) -> int:
        return _virtual_listimages_len(self)

    def keys(self) -> list[str | list[str]]:
        return _virtual_listimages_keys(self)

    def items(self) -> list[tuple[str | list[str], ImageFile]]:
        return _virtual_listimages_items(self)

    def __getitem__(
        self, index: int | slice | str | list[str] | tuple[str]
    ) -> ImageFile | Sequence[ImageFile]:
        return _virtual_listimages_getitem(self, index)

    def __iter__(self) -> Iterator[ImageFile]:
        for image_id in self.ids_function():
            yield self.get_function(image_id)

    def __str__(self) -> str:
        return _virtual_listimages_str(self)


class PageObject(DictionaryObject):
    """
    PageObject represents a single page within a PDF file.

    Typically these objects will be created by accessing the
    :attr:`pages<pypdf.PdfReader.pages>` property of the
    :class:`PdfReader<pypdf.PdfReader>` class, but it is
    also possible to create an empty page with the
    :meth:`create_blank_page()<pypdf._page.PageObject.create_blank_page>` static method.

    Args:
        pdf: PDF file the page belongs to.
        indirect_reference: Stores the original indirect reference to
            this object in its source PDF

    """

    original_page: "PageObject"  # very local use in writer when appending

    def __init__(
        self,
        pdf: PdfCommonDocProtocol | None = None,
        indirect_reference: IndirectObject | None = None,
    ) -> None:
        return _page_init_impl(self, pdf, indirect_reference)

    def hash_bin(self) -> int:
        """
        Used to detect modified object.

        Note: this function is overloaded to return the same results
        as a DictionaryObject.

        Returns:
            Hash considering type and value.

        """
        return _hash_bin_impl(self)

    def hash_value_data(self) -> bytes:
        return _hash_value_data_impl(self)

    @property
    def user_unit(self) -> float:
        """
        A read-only positive number giving the size of user space units.

        It is in multiples of 1/72 inch. Hence a value of 1 means a user
        space unit is 1/72 inch, and a value of 3 means that a user
        space unit is 3/72 inch.
        """
        return _user_unit_impl(self)

    @staticmethod
    def create_blank_page(
        pdf: PdfCommonDocProtocol | None = None,
        width: float | Decimal | None = None,
        height: float | Decimal | None = None,
    ) -> "PageObject":
        """
        Return a new blank page.

        If ``width`` or ``height`` is ``None``, try to get the page size
        from the last page of *pdf*.

        Args:
            pdf: PDF file the page is within.
            width: The width of the new page expressed in default user
                space units.
            height: The height of the new page expressed in default user
                space units.

        Returns:
            The new blank page

        Raises:
            PageSizeNotDefinedError: if ``pdf`` is ``None`` or contains
                no page

        """
        return _create_blank_page_impl(pdf, width, height)

    def _get_ids_image(
        self,
        obj: DictionaryObject | None = None,
        ancest: list[str] | None = None,
        call_stack: list[Any] | None = None,
    ) -> list[str | list[str]]:
        return _get_ids_image_impl(self, obj, ancest, call_stack)

    def _get_image(
        self,
        image_id: str | list[str] | tuple[str],
        obj: DictionaryObject | None = None,
    ) -> ImageFile:
        return _get_image_impl(self, image_id, obj)

    @property
    def images(self) -> VirtualListImages:
        """
        Read-only property emulating a list of images on a page.

        Get a list of all images on the page. The key can be:
        - A string (for the top object)
        - A tuple (for images within XObject forms)
        - An integer

        Examples:
            * `reader.pages[0].images[0]`        # return first image
            * `reader.pages[0].images['/I0']`    # return image '/I0'
            * `reader.pages[0].images['/TP1','/Image1']` # return image '/Image1' within '/TP1' XObject form
            * `for img in reader.pages[0].images:` # loops through all objects

        Example usage:

            >>> from pypdf import PdfWriter
            >>> writer = PdfWriter()
            >>> page = writer.add_blank_page(800, 600)
            >>> images = page.images

        """
        return _images_impl(self)

    @property
    def inline_images(self) -> NoReturn:
        """
        Return only inline images from the page.

        .. deprecated::
            Use :attr:`images` and filter by :attr:`ImageFile.is_inline` instead.
            This property has been removed in pypdf 7.0.

        Examples:
            >>> from pypdf import PdfWriter
            >>> writer = PdfWriter()
            >>> page = writer.add_blank_page(800,600)
            >>> for image_name, image_file in page.images.items():
            ...     if image_file.is_inline:
            ...         print(f"{image_name} is inline")
        """
        return _inline_images_get()

    @inline_images.setter
    def inline_images(self, value: dict[str, ImageFile] | None) -> NoReturn:
        return _inline_images_set()

    def _translate_value_inline_image(self, k: str, v: PdfObject) -> PdfObject:
        """Translate values used in inline image"""
        return _translate_value_inline_image_impl(self, k, v)

    def _parse_images_from_content_stream(self) -> dict[str, ImageFile | None]:
        """Load images from content stream. Includes both inline images and Do-referenced images.

        This method scans the page content stream and extracts:

        1. **Inline images** (~0~, ~1~...): Embedded directly in content stream via BI/EI operators
           - is_inline=True, is_displayed=True, indirect_reference=None

        2. **Do-referenced objects** (/Im0, /Im1..., /Form1...): Referenced via "Do" operator
           - is_inline=False, is_displayed=True, indirect_reference=<image object>

        3. **Pure XObject images** (/I0, /Image1...): Defined in Resources only (not in content stream)
           - is_inline=False, is_displayed=False, indirect_reference=<image object>

        Returns:
            Dictionary mapping names to ImageFile instances (inline) or None (Do-referenced).
        """
        return _parse_images_from_content_stream_impl(self)

    @property
    def rotation(self) -> int:
        """
        The visual rotation of the page.

        This number has to be a multiple of 90 degrees: 0, 90, 180, or 270 are
        valid values. This property does not affect ``/Contents``.
        """
        return _rotation_get(self)

    @rotation.setter
    def rotation(self, r: float) -> None:
        return _rotation_set(self, r)

    def transfer_rotation_to_content(self) -> None:
        """
        Apply the rotation of the page to the content and the media/crop/...
        boxes.

        It is recommended to apply this function before page merging.
        """
        return _transfer_rotation_to_content_impl(self)

    def rotate(self, angle: int) -> "PageObject":
        """
        Rotate a page clockwise by increments of 90 degrees.

        Args:
            angle: Angle to rotate the page. Must be an increment of 90 deg.

        Returns:
            The rotated PageObject

        """
        return _rotate_impl(self, angle)

    def _merge_resources(
        self,
        res1: DictionaryObject,
        res2: DictionaryObject,
        resource: Any,
        new_res1: bool = True,
    ) -> tuple[dict[str, Any], dict[str, Any]]:
        return _merge_resources_impl(self, res1, res2, resource, new_res1)

    @staticmethod
    def _content_stream_rename(
        stream: ContentStream,
        rename: dict[Any, Any],
        pdf: PdfCommonDocProtocol | None,
    ) -> ContentStream:
        return _content_stream_rename_impl(stream, rename, pdf)

    @staticmethod
    def _add_transformation_matrix(
        contents: Any,
        pdf: PdfCommonDocProtocol | None,
        ctm: CompressedTransformationMatrix,
    ) -> ContentStream:
        """Add transformation matrix at the beginning of the given contents stream."""
        return _add_transformation_matrix_impl(contents, pdf, ctm)

    def _get_contents_as_bytes(self) -> bytes | None:
        """
        Return the page contents as bytes.

        Returns:
            The ``/Contents`` object as bytes, or ``None`` if it doesn't exist.

        """
        return _get_contents_as_bytes_impl(self)

    def get_contents(self) -> ContentStream | None:
        """
        Access the page contents.

        Returns:
            The ``/Contents`` object, or ``None`` if it does not exist.
            ``/Contents`` is optional, as described in §7.7.3.3 of the PDF Reference.

        """
        return _get_contents_impl(self)

    def replace_contents(
        self, content: ContentStream | EncodedStreamObject | ArrayObject | None
    ) -> None:
        """
        Replace the page contents with the new content and nullify old objects
        Args:
            content: new content; if None delete the content field.
        """
        return _replace_contents_impl(self, content)

    def merge_page(
        self, page2: "PageObject", expand: bool = False, over: bool = True
    ) -> None:
        """
        Merge the content streams of two pages into one.

        Resource references (e.g. fonts) are maintained from both pages.
        The mediabox, cropbox, etc of this page are not altered.
        The parameter page's content stream will
        be added to the end of this page's content stream,
        meaning that it will be drawn after, or "on top" of this page.

        Args:
            page2: The page to be merged into this one. Should be
                an instance of :class:`PageObject<PageObject>`.
            over: set the page2 content over page1 if True (default) else under
            expand: If True, the current page dimensions will be
                expanded to accommodate the dimensions of the page to be merged.

        """
        return _merge_page_impl(self, page2, expand, over)

    def _merge_page(
        self,
        page2: "PageObject",
        page2_transformation: Callable[[Any], ContentStream] | None = None,
        ctm: CompressedTransformationMatrix | None = None,
        over: bool = True,
        expand: bool = False,
    ) -> None:
        return _merge_page_inner_impl(
            self, page2, page2_transformation, ctm, over, expand
        )

    def _merge_page_writer(
        self,
        page2: "PageObject",
        page2transformation: Callable[[Any], ContentStream] | None = None,
        ctm: CompressedTransformationMatrix | None = None,
        over: bool = True,
        expand: bool = False,
    ) -> None:
        return _merge_page_writer_impl(
            self, page2, page2transformation, ctm, over, expand
        )

    def _expand_mediabox(
        self, page2: "PageObject", ctm: CompressedTransformationMatrix | None
    ) -> None:
        return _expand_mediabox_impl(self, page2, ctm)

    def merge_transformed_page(
        self,
        page2: "PageObject",
        ctm: CompressedTransformationMatrix | Transformation,
        over: bool = True,
        expand: bool = False,
    ) -> None:
        """
        Similar to :meth:`~pypdf._page.PageObject.merge_page`, but a transformation
        matrix is applied to the merged stream.

        Args:
          page2: The page to be merged into this one.
          ctm: a 6-element tuple containing the operands of the
                 transformation matrix
          over: set the page2 content over page1 if True (default) else under
          expand: Whether the page should be expanded to fit the dimensions
            of the page to be merged.

        """
        return _merge_transformed_page_impl(self, page2, ctm, over, expand)

    def merge_scaled_page(
        self, page2: "PageObject", scale: float, over: bool = True, expand: bool = False
    ) -> None:
        """
        Similar to :meth:`~pypdf._page.PageObject.merge_page`, but the stream to be merged
        is scaled by applying a transformation matrix.

        Args:
          page2: The page to be merged into this one.
          scale: The scaling factor
          over: set the page2 content over page1 if True (default) else under
          expand: Whether the page should be expanded to fit the
            dimensions of the page to be merged.

        """
        return _merge_scaled_page_impl(self, page2, scale, over, expand)

    def merge_rotated_page(
        self,
        page2: "PageObject",
        rotation: float,
        over: bool = True,
        expand: bool = False,
    ) -> None:
        """
        Similar to :meth:`~pypdf._page.PageObject.merge_page`, but the stream to be merged
        is rotated by applying a transformation matrix.

        Args:
          page2: The page to be merged into this one.
          rotation: The angle of the rotation, in degrees
          over: set the page2 content over page1 if True (default) else under
          expand: Whether the page should be expanded to fit the
            dimensions of the page to be merged.

        """
        return _merge_rotated_page_impl(self, page2, rotation, over, expand)

    def merge_translated_page(
        self,
        page2: "PageObject",
        tx: float,
        ty: float,
        over: bool = True,
        expand: bool = False,
    ) -> None:
        """
        Similar to :meth:`~pypdf._page.PageObject.merge_page`, but the stream to be
        merged is translated by applying a transformation matrix.

        Args:
          page2: the page to be merged into this one.
          tx: The translation on X axis
          ty: The translation on Y axis
          over: set the page2 content over page1 if True (default) else under
          expand: Whether the page should be expanded to fit the
            dimensions of the page to be merged.

        """
        return _merge_translated_page_impl(self, page2, tx, ty, over, expand)

    def add_transformation(
        self,
        ctm: Transformation | CompressedTransformationMatrix,
        expand: bool = False,
    ) -> None:
        """
        Apply a transformation matrix to the page.

        Args:
            ctm: A 6-element tuple containing the operands of the
                transformation matrix. Alternatively, a
                :py:class:`Transformation<pypdf.Transformation>`
                object can be passed.

        See :doc:`/user/cropping-and-transforming`.

        """
        return _page_add_transformation_impl(self, ctm, expand)

    def scale(self, sx: float, sy: float) -> None:
        """
        Scale a page by the given factors by applying a transformation matrix
        to its content and updating the page size.

        This updates the various page boundaries (bleedbox, trimbox, etc.)
        and the contents of the page.

        Args:
            sx: The scaling factor on horizontal axis.
            sy: The scaling factor on vertical axis.

        """
        return _scale_impl(self, sx, sy)

    def scale_by(self, factor: float) -> None:
        """
        Scale a page by the given factor by applying a transformation matrix to
        its content and updating the page size.

        Args:
            factor: The scaling factor (for both X and Y axis).

        """
        return _scale_by_impl(self, factor)

    def scale_to(self, width: float, height: float) -> None:
        """
        Scale a page to the specified dimensions by applying a transformation
        matrix to its content and updating the page size.

        Args:
            width: The new width.
            height: The new height.

        """
        return _scale_to_impl(self, width, height)

    def compress_content_streams(self, level: int = -1) -> None:
        """
        Compress the size of this page by joining all content streams and
        applying a FlateDecode filter.

        However, it is possible that this function will perform no action if
        content stream compression becomes "automatic".
        """
        return _compress_content_streams_impl(self, level)

    @property
    def page_number(self) -> int | None:
        """
        Read-only property which returns the page number within the PDF file.

        Returns:
            Page number; None if the page is not attached to a PDF.

        """
        return _page_number_impl(self)

    def _debug_for_extract(self) -> str:  # pragma: no cover
        return _debug_for_extract_impl(self)

    def _extract_text(
        self,
        obj: DictionaryObject,
        pdf: Any,
        orientations: tuple[int, ...] = (0, 90, 180, 270),
        space_width: float = 200.0,
        content_key: str | None = PageAttributes.CONTENTS,
        visitor_operand_before: Callable[[Any, Any, Any, Any], None] | None = None,
        visitor_operand_after: Callable[[Any, Any, Any, Any], None] | None = None,
        visitor_text: Callable[[Any, Any, Any, Any, Any], None] | None = None,
        *,
        known_ids: set[int] | None = None,
        traversal_state: _TraversalState | None = None,
    ) -> str:
        """
        See extract_text for most arguments.

        Args:
            content_key: indicate the default key where to extract data
                None = the object; this allows reusing the function on an XObject
                default = "/Content"

        """
        return _extract_text_impl(
            self,
            obj,
            pdf,
            orientations,
            space_width,
            content_key,
            visitor_operand_before,
            visitor_operand_after,
            visitor_text,
            known_ids,
            traversal_state,
        )

    def _extract_text__xform(
        self,
        *,
        resources_dict: DictionaryObject,
        operands: Any,
        orientations: tuple[int, ...] = (0, 90, 180, 270),
        space_width: float = 200.0,
        visitor_operand_before: Callable[[Any, Any, Any, Any], None] | None = None,
        visitor_operand_after: Callable[[Any, Any, Any, Any], None] | None = None,
        visitor_text: Callable[[Any, Any, Any, Any, Any], None] | None = None,
        known_ids: set[int],
        traversal_state: _TraversalState,
    ) -> str | None:
        return _extract_text_xform_impl(
            self,
            resources_dict,
            operands,
            orientations,
            space_width,
            visitor_operand_before,
            visitor_operand_after,
            visitor_text,
            known_ids,
            traversal_state,
        )

    def _layout_mode_fonts(self) -> dict[str, Font]:
        """
        Get fonts formatted for "layout" mode text extraction.

        Returns:
            Dict[str, Font]: dictionary of Font instances keyed by font name

        """
        return _layout_mode_fonts_impl(self)

    def _layout_mode_text(
        self,
        space_vertically: bool = True,
        scale_weight: float = 1.25,
        strip_rotated: bool = True,
        debug_path: Path | None = None,
        font_height_weight: float = 1,
    ) -> str:
        """
        Get text preserving fidelity to source PDF text layout.

        Args:
            space_vertically: include blank lines inferred from y distance + font
                height. Defaults to True.
            scale_weight: multiplier for string length when calculating weighted
                average character width. Defaults to 1.25.
            strip_rotated: Removes text that is rotated w.r.t. to the page from
                layout mode output. Defaults to True.
            debug_path (Path | None): if supplied, must target a directory.
                creates the following files with debug information for layout mode
                functions if supplied:
                  - fonts.json: output of self._layout_mode_fonts
                  - tjs.json: individual text render ops with corresponding transform matrices
                  - bts.json: text render ops left justified and grouped by BT/ET operators
                  - bt_groups.json: BT/ET operations grouped by rendered y-coord (aka lines)
                Defaults to None.
            font_height_weight: multiplier for font height when calculating
                blank lines. Defaults to 1.

        Returns:
            str: multiline string containing page text in a fixed width format that
                closely adheres to the rendered layout in the source pdf.

        """
        return _layout_mode_text_impl(
            self,
            space_vertically,
            scale_weight,
            strip_rotated,
            debug_path,
            font_height_weight,
        )

    def extract_text(
        self,
        *args: Any,
        orientations: int | tuple[int, ...] = (0, 90, 180, 270),
        space_width: float = 200.0,
        visitor_operand_before: Callable[[Any, Any, Any, Any], None] | None = None,
        visitor_operand_after: Callable[[Any, Any, Any, Any], None] | None = None,
        visitor_text: Callable[[Any, Any, Any, Any, Any], None] | None = None,
        extraction_mode: Literal["plain", "layout"] = "plain",
        **kwargs: Any,
    ) -> str:
        """
        Locate all text drawing commands, in the order they are provided in the
        content stream, and extract the text.

        This works well for some PDF files, but poorly for others, depending on
        the generator used. This will be refined in the future.

        Do not rely on the order of text coming out of this function, as it
        will change if this function is made more sophisticated.

        Arabic and Hebrew are extracted in the correct order.
        If required a custom RTL range of characters can be defined;
        see function set_custom_rtl.

        Additionally you can provide visitor methods to get informed on all
        operations and all text objects.
        For example in some PDF files this can be useful to parse tables.

        Args:
            orientations: list of orientations extract_text will look for
                default = (0, 90, 180, 270)
                note: currently only 0 (up),90 (turned left), 180 (upside down),
                270 (turned right)
                Silently ignored in "layout" mode.
            space_width: force default space width
                if not extracted from font (default: 200)
                Silently ignored in "layout" mode.
            visitor_operand_before: function to be called before processing an operation.
                It has four arguments: operator, operand-arguments,
                current transformation matrix and text matrix.
                Ignored with a warning in "layout" mode.
            visitor_operand_after: function to be called after processing an operation.
                It has four arguments: operator, operand-arguments,
                current transformation matrix and text matrix.
                Ignored with a warning in "layout" mode.
            visitor_text: function to be called when extracting some text at some position.
                It has five arguments: text, current transformation matrix,
                text matrix, font-dictionary and font-size.
                The font-dictionary may be None in case of unknown fonts.
                If not None it may e.g. contain key "/BaseFont" with value "/Arial,Bold".
                Ignored with a warning in "layout" mode.
            extraction_mode (Literal["plain", "layout"]): "plain" for legacy functionality,
                "layout" for experimental layout mode functionality.
                NOTE: orientations, space_width, and visitor_* parameters are NOT respected
                in "layout" mode.

        kwargs:
            layout_mode_space_vertically (bool): include blank lines inferred from
                y distance + font height. Defaults to True.
            layout_mode_scale_weight (float): multiplier for string length when calculating
                weighted average character width. Defaults to 1.25.
            layout_mode_strip_rotated (bool): layout mode does not support rotated text.
                Set to False to include rotated text anyway. If rotated text is discovered,
                layout will be degraded and a warning will result. Defaults to True.
            layout_mode_debug_path (Path | None): if supplied, must target a directory.
                creates the following files with debug information for layout mode
                functions if supplied:

                  - fonts.json: output of self._layout_mode_fonts
                  - tjs.json: individual text render ops with corresponding transform matrices
                  - bts.json: text render ops left justified and grouped by BT/ET operators
                  - bt_groups.json: BT/ET operations grouped by rendered y-coord (aka lines)
            layout_mode_font_height_weight (float): multiplier for font height when calculating
                blank lines. Defaults to 1.

        Returns:
            The extracted text

        """
        return _extract_text_public_impl(
            self,
            args,
            orientations,
            space_width,
            visitor_operand_before,
            visitor_operand_after,
            visitor_text,
            extraction_mode,
            kwargs,
        )

    def extract_xform_text(
        self,
        xform: EncodedStreamObject,
        orientations: tuple[int, ...] = (0, 90, 270, 360),
        space_width: float = 200.0,
        visitor_operand_before: Callable[[Any, Any, Any, Any], None] | None = None,
        visitor_operand_after: Callable[[Any, Any, Any, Any], None] | None = None,
        visitor_text: Callable[[Any, Any, Any, Any, Any], None] | None = None,
        *,
        known_ids: set[int] | None = None,
        traversal_state: Any | None = None,
    ) -> str:
        """
        Extract text from an XObject.

        Args:
            xform:
            orientations:
            space_width:  force default space width (if not extracted from font (default 200)
            visitor_operand_before:
            visitor_operand_after:
            visitor_text:
            known_ids:
            traversal_state:

        Returns:
            The extracted text

        """
        return _extract_xform_text_impl(
            self,
            xform,
            orientations,
            space_width,
            visitor_operand_before,
            visitor_operand_after,
            visitor_text,
            known_ids,
            traversal_state,
        )

    def _get_fonts(self) -> tuple[set[str], set[str]]:
        """
        Get the names of embedded fonts and unembedded fonts.

        Returns:
            A tuple (set of embedded fonts, set of unembedded fonts)

        """
        return _get_fonts_impl(self)

    mediabox = _create_rectangle_accessor(PageAttributes.MEDIABOX, ())
    """A :class:`RectangleObject<pypdf.generic.RectangleObject>`, expressed in
    default user space units, defining the boundaries of the physical medium on
    which the page is intended to be displayed or printed."""

    cropbox = _create_rectangle_accessor("/CropBox", (PageAttributes.MEDIABOX,))
    """
    A :class:`RectangleObject<pypdf.generic.RectangleObject>`, expressed in
    default user space units, defining the visible region of default user
    space.

    When the page is displayed or printed, its contents are to be clipped
    (cropped) to this rectangle and then imposed on the output medium in some
    implementation-defined manner. Default value: same as
    :attr:`mediabox<mediabox>`.
    """

    bleedbox = _create_rectangle_accessor("/BleedBox", ("/CropBox", PageAttributes.MEDIABOX))
    """A :class:`RectangleObject<pypdf.generic.RectangleObject>`, expressed in
    default user space units, defining the region to which the contents of the
    page should be clipped when output in a production environment."""

    trimbox = _create_rectangle_accessor("/TrimBox", ("/CropBox", PageAttributes.MEDIABOX))
    """A :class:`RectangleObject<pypdf.generic.RectangleObject>`, expressed in
    default user space units, defining the intended dimensions of the finished
    page after trimming."""

    artbox = _create_rectangle_accessor("/ArtBox", ("/CropBox", PageAttributes.MEDIABOX))
    """A :class:`RectangleObject<pypdf.generic.RectangleObject>`, expressed in
    default user space units, defining the extent of the page's meaningful
    content as intended by the page's creator."""

    @property
    def annotations(self) -> ArrayObject | None:
        return _annotations_get_impl(self)

    @annotations.setter
    def annotations(self, value: ArrayObject | None) -> None:
        """
        Set the annotations array of the page.

        Typically you do not want to set this value, but append to it.
        If you append to it, remember to add the object first to the writer
        and only add the indirect object.
        """
        return _annotations_set_impl(self, value)

    def add_action(self, trigger: PageTrigger, action: Action) -> None:
        """
        Add an action which will launch on the given trigger event of this page.

        Args:
            trigger: The action trigger to use.
            action: The action to be done.

        Example:
            >>> from pypdf import PdfWriter
            >>> from pypdf.actions import JavaScript, PageTrigger
            >>> writer = PdfWriter()
            >>> page = writer.add_blank_page(595, 842)
            >>> # Display the page number when the page is opened
            >>> page.add_action(PageTrigger("open"), JavaScript("app.alert('This is page ' + this.pageNum);"))
            >>> # Display the page number when the page is closed
            >>> page.add_action(PageTrigger("close"), JavaScript("app.alert('This is page ' + this.pageNum);"))
        """
        return _add_action_impl(self, trigger, action)

    def delete_action(self, trigger: PageTrigger) -> None:
        """
        Delete all actions associated with an open or close trigger event of this page.

        Args:
            trigger: An open or close trigger.

        Example:
            >>> from pypdf import PdfWriter
            >>> from pypdf.actions import JavaScript, PageTrigger
            >>> writer = PdfWriter()
            >>> page = writer.add_blank_page(595, 842)
            >>> page.add_action(PageTrigger("open"), JavaScript("app.alert('This is page ' + this.pageNum);"))
            >>> page.add_action(PageTrigger("close"), JavaScript("app.alert('This is page ' + this.pageNum);"))
            >>> # Delete all actions triggered by a page open
            >>> page.delete_action(PageTrigger("open"))
            >>> # Delete all actions triggered by a page close
            >>> page.delete_action(PageTrigger("close"))
        """
        return _delete_action_impl(self, trigger)


class _VirtualList(Sequence[PageObject]):
    def __init__(
        self,
        length_function: Callable[[], int],
        get_function: Callable[[int], PageObject],
    ) -> None:
        self.length_function = length_function
        self.get_function = get_function
        self.current = -1

    def __len__(self) -> int:
        return _virtual_list_len(self)

    def __getitem__(
        self, index: int | slice
    ) -> PageObject | Sequence[PageObject]:
        return _virtual_list_getitem(self, index)

    def __delitem__(self, index: int | slice) -> None:
        return _virtual_list_delitem(self, index)

    def __iter__(self) -> Iterator[PageObject]:
        for i in range(len(self)):
            yield self[i]

    def __str__(self) -> str:
        return _virtual_list_str(self)


def _get_fonts_walk(
    obj: DictionaryObject,
    fnt: set[str],
    emb: set[str],
) -> tuple[set[str], set[str]]:
    """
    Get the set of all fonts and all embedded fonts.

    Args:
        obj: Page resources dictionary
        fnt: font
        emb: embedded fonts

    Returns:
        A tuple (fnt, emb)

    If there is a key called 'BaseFont', that is a font that is used in the document.
    If there is a key called 'FontName' and another key in the same dictionary object
    that is called 'FontFilex' (where x is null, 2, or 3), then that fontname is
    embedded.

    We create and add to two sets, fnt = fonts used and emb = fonts embedded.

    """
    return _get_fonts_walk_impl(obj, fnt, emb)
