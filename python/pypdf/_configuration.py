"""Configuration value handling.

AUTO-GENERATED stub for the Rust port (upstream pypdf 7.1.1, module ``pypdf._configuration``).
Replaced by a Rust-backed implementation in phase later.
"""

from __future__ import annotations

import dataclasses
import functools
import shutil
from collections.abc import Iterator
from contextlib import contextmanager
from contextvars import ContextVar
from typing import Any, Literal
def _determine_jbig2dec_binary(*args, **kwargs):
    return None  # inert stub: pypdf._configuration._determine_jbig2dec_binary
class Configuration:
    """Stub for pypdf._configuration.Configuration."""
    maximum_declared_stream_length = None
    array_based_stream_maximum_output_length = None
    array_maximum_element_count = None
    dictionary_maximum_element_count = None
    jbig2_maximum_output_length = None
    lzw_maximum_output_length = None
    run_length_maximum_output_length = None
    zlib_maximum_output_length = None
    zlib_maximum_recovery_input_length = None
    brotli_maximum_output_length = None
    flate_maximum_columns = None
    flate_maximum_row_length = None
    image_maximum_buffer_size = None
    stream_filters_maximum_length = None
    stream_decoding_work_maximum_length = None
    stream_parsing_work_maximum_count = None
    xmp_maximum_input_length = None
    xmp_maximum_element_count = None
    outline_maximum_entries = None
    outline_maximum_depth = None
    page_tree_maximum_entries = None
    page_tree_maximum_depth = None
    page_tree_maximum_page_entries = None
    form_fields_maximum_entries = None
    form_fields_maximum_depth = None
    named_destinations_maximum_entries = None
    named_destinations_maximum_depth = None
    xform_maximum_invocations_per_extraction = None
    xfa_decoding_work_maximum_length = None
    jbig2dec_binary = None
    page_merge_box = None
    disable_legacy_handling = None
    def with_overwrites(self, *args, **kwargs):
        return None  # inert stub
DEFAULT_CONFIGURATION = None
CURRENT_CONFIGURATION = None
def get_configuration(*args, **kwargs):
    return None  # inert stub: pypdf._configuration.get_configuration
def _build_configuration(*args, **kwargs):
    return None  # inert stub: pypdf._configuration._build_configuration
def overwrite_configuration(*args, **kwargs):
    return None  # inert stub: pypdf._configuration.overwrite_configuration
def apply_configuration(*args, **kwargs):
    return None  # inert stub: pypdf._configuration.apply_configuration
LEGACY_NAME_MAPPING = {}
def apply_legacy_configuration(*args, **kwargs):
    return None  # inert stub: pypdf._configuration.apply_legacy_configuration
