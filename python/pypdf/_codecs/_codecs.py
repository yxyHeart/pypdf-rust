"""This module is for codecs only.

While the codec implementation can contain details of the PDF specification,
the module should not do any PDF parsing.

Rust-backed module: the implementation lives in
``crates/pypdf-py/src/modules/codecs_mod.rs``.
"""

from __future__ import annotations

from pypdf._pypdf import Codec, LzwCodec

__all__ = ["Codec", "LzwCodec"]
