"""Utility functions for PDF library.

Rust-backed module: the implementation lives in
``crates/pypdf-py/src/modules/utils_mod.rs``.  Two Python-protocol helpers stay
in this shim because they operate on Python callables/signatures.
"""

from __future__ import annotations

import functools
import inspect
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
    File,
    Version,
    _TraversalState,
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
