"""Utility functions for PDF library.

Rust-backed module: the implementation lives in
``crates/pypdf-py/src/modules/utils_mod.rs``.  Two Python-protocol helpers stay
in this shim because they operate on Python callables/signatures.
"""

from __future__ import annotations

import functools
import inspect
import re
import warnings
from typing import Any, Callable, TypeVar, cast

from pypdf._pypdf import (
    __author__,
    __author_email__,
    TransformationMatrixType,
    CompressedTransformationMatrix,
    StreamType,
    BinaryStreamType,
    StrByteType,
    parse_iso8824_date,
    format_iso8824_date,
    _get_max_pdf_version_header,
    WHITESPACES,
    WHITESPACES_AS_BYTES,
    WHITESPACES_AS_REGEXP,
    read_until_whitespace,
    read_non_whitespace,
    skip_over_whitespace,
    check_if_whitespace_only,
    NEUTRAL_CHARACTER_RANGES,
    is_char_neutral,
    RTL_CHARACTER_RANGES,
    is_char_rtl,
    skip_over_comment,
    skip_over_comments,
    read_until_regex,
    read_block_backwards,
    read_previous_line,
    matrix_multiply,
    mark_location,
    deprecate,
    deprecation,
    deprecate_with_replacement,
    deprecation_with_replacement,
    _P,
    _R,
    deprecate_no_replacement,
    deprecation_no_replacement,
    logger_error,
    logger_warning,
    rename_kwargs,
    _human_readable_bytes,
    get_stream_size,
    get_stream_data,
)

def deprecate_positional_arguments(removed_in: str) -> Callable[[Callable[_P, _R]], Callable[_P, _R]]:
    """
    Accept positional arguments for keyword-only parameters, but issue a warning.

    Use it for a function whose parameters became keyword-only. The positional
    arguments that exceed the positional parameters are passed to the
    keyword-only parameters in the order of the signature.

    Args:
        removed_in: The version in which positional arguments will raise an error.

    Returns:
        The decorator.

    """
    def decorator(func: Callable[_P, _R]) -> Callable[_P, _R]:
        parameters = inspect.signature(func).parameters.values()
        positional_count = sum(
            parameter.kind in (parameter.POSITIONAL_ONLY, parameter.POSITIONAL_OR_KEYWORD)
            for parameter in parameters
        )
        keyword_only_names = [parameter.name for parameter in parameters if parameter.kind == parameter.KEYWORD_ONLY]

        @functools.wraps(func)
        def wrapper(*args: Any, **kwargs: Any) -> Any:
            extra_args = args[positional_count:]
            if extra_args:
                if len(extra_args) > len(keyword_only_names):
                    raise TypeError(
                        f"{func.__qualname__}() takes {positional_count + len(keyword_only_names)} "
                        f"positional arguments but {len(args)} were given"
                    )
                for name, value in zip(keyword_only_names, extra_args):
                    if name in kwargs:
                        raise TypeError(f"{func.__qualname__}() got multiple values for argument '{name}'")
                    kwargs[name] = value
                deprecate_with_replacement(
                    f"Passing positional arguments to {func.__qualname__}()", "keyword arguments", removed_in
                )
            return func(*args[:positional_count], **kwargs)

        return cast(Callable[_P, _R], wrapper)

    return decorator


class classproperty:  # noqa: N801
    """
    Decorator that converts a method with a single cls argument into a property
    that can be accessed directly from the class.
    """

    def __init__(self, method=None) -> None:  # type: ignore[no-untyped-def]  # noqa: ANN001
        self.fget = method

    def __get__(self, instance, cls=None) -> Any:  # type: ignore[no-untyped-def]  # noqa: ANN001
        return self.fget(cls)

    def getter(self, method) -> Self:  # type: ignore[no-untyped-def]  # noqa: ANN001
        self.fget = method
        return self


# --- upstream module-namespace parity (validation §4.1) -----------------------
from collections.abc import Callable as Callable  # noqa: F811 - upstream imports it from here
from dataclasses import dataclass as dataclass
from datetime import datetime as datetime, timezone as timezone
from io import DEFAULT_BUFFER_SIZE as DEFAULT_BUFFER_SIZE
from os import SEEK_CUR as SEEK_CUR
from re import Pattern as Pattern
from typing import IO as IO, NoReturn as NoReturn, ParamSpec as ParamSpec, Self as Self, TypeAlias as TypeAlias

from pypdf.errors import (  # noqa: F401 - re-exported like upstream
    STREAM_TRUNCATED_PREMATURELY as STREAM_TRUNCATED_PREMATURELY,
    DeprecationError as DeprecationError,
    LimitReachedError as LimitReachedError,
    PdfStreamError as PdfStreamError,
)

@dataclass
class File:
    from .generic import IndirectObject  # noqa: PLC0415

    name: str = ""
    """
    Filename as identified within the PDF file.
    """
    data: bytes = b""
    """
    Data as bytes.
    """
    indirect_reference: IndirectObject | None = None
    """
    Reference to the object storing the stream.
    """

    def __str__(self) -> str:
        return f"{self.__class__.__name__}(name={self.name}, data: {_human_readable_bytes(len(self.data))})"

    def __repr__(self) -> str:
        return self.__str__()[:-1] + f", hash: {hash(self.data)})"


@functools.total_ordering
class Version:
    COMPONENT_PATTERN = re.compile(r"^(\d+)(.*)$")

    def __init__(self, version_str: str) -> None:
        self.version_str = version_str
        self.components = self._parse_version(version_str)

    def _parse_version(self, version_str: str) -> list[tuple[int, str]]:
        components = version_str.split(".")
        parsed_components = []
        for component in components:
            match = Version.COMPONENT_PATTERN.match(component)
            if not match:
                parsed_components.append((0, component))
                continue
            integer_prefix = match.group(1)
            suffix = match.group(2)
            parsed_components.append((int(integer_prefix), suffix))
        return parsed_components

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, Version):
            return False
        return self.components == other.components

    def __hash__(self) -> int:
        # Convert to tuple as lists cannot be hashed.
        return hash((self.__class__, tuple(self.components)))

    def __lt__(self, other: Any) -> bool:
        if not isinstance(other, Version):
            raise ValueError(f"Version cannot be compared against {type(other)}")

        for self_component, other_component in zip(self.components, other.components):
            self_value, self_suffix = self_component
            other_value, other_suffix = other_component

            if self_value < other_value:
                return True
            if self_value > other_value:
                return False

            if self_suffix < other_suffix:
                return True
            if self_suffix > other_suffix:
                return False

        return len(self.components) < len(other.components)


@dataclass
class _TraversalState:
    """Sometimes we need mutable objects which just count something."""
    entry_count: int = 0
    has_logged: bool = False


# --- public wrappers carrying upstream defaults/annotations -------------------
_is_char_rtl_impl = is_char_rtl
_is_char_neutral_impl = is_char_neutral


def is_char_rtl(char: str, custom_rtl_min: str = "", custom_rtl_max: str = "") -> bool:
    """Check if a character is part of RTL character ranges"""
    return _is_char_rtl_impl(char, custom_rtl_min, custom_rtl_max)


def is_char_neutral(char: str, custom_special_characters: str = "") -> bool:
    """Check if a character is part of neutral character ranges"""
    return _is_char_neutral_impl(char, custom_special_characters)

# --- upstream type-annotation parity (validation §4.2) -----------------------
from pypdf._api_annotations import apply_module as _apply_api_annotations

_apply_api_annotations(globals())
globals().pop("annotations", None)  # `from __future__ import annotations` binds this name; upstream modules do not have it
