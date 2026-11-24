# 符号覆盖报告（Symbol Coverage）

> 由 `tools/manifest/check.py` 从 `docs/symbols.json` 与 Rust 源码中的
> `// @pypdf:` / `// @pypdf-module:` 注解自动生成。门禁 A6 要求 100%。

总计：**1902/1934**（98.35%）

| 模块 | 已覆盖 | 总数 | 覆盖 | 实现方式 |
|---|---:|---:|---:|---|
| `pypdf` | 4 | 4 | 100% | python |
| `pypdf._cmap` | 20 | 20 | 100% | rust |
| `pypdf._codecs` | 8 | 8 | 100% | python |
| `pypdf._codecs._codecs` | 18 | 18 | 100% | rust |
| `pypdf._codecs.adobe_glyphs` | 2 | 2 | 100% | python |
| `pypdf._codecs.core_font_metrics` | 1 | 1 | 100% | python |
| `pypdf._codecs.pdfdoc` | 1 | 1 | 100% | python |
| `pypdf._codecs.std` | 1 | 1 | 100% | python |
| `pypdf._codecs.symbol` | 1 | 1 | 100% | python |
| `pypdf._codecs.zapfding` | 1 | 1 | 100% | python |
| `pypdf._configuration` | 43 | 43 | 100% | python |
| `pypdf._crypt_providers` | 1 | 1 | 100% | python |
| `pypdf._crypt_providers._base` | 4 | 4 | 100% | rust |
| `pypdf._crypt_providers._cryptography` | 18 | 18 | 100% | rust |
| `pypdf._crypt_providers._fallback` | 16 | 16 | 100% | rust |
| `pypdf._crypt_providers._pycryptodome` | 15 | 15 | 100% | rust |
| `pypdf._doc_common` | 89 | 89 | 100% | rust |
| `pypdf._encryption` | 58 | 58 | 100% | rust |
| `pypdf._page` | 108 | 108 | 100% | stub |
| `pypdf._page_labels` | 36 | 36 | 100% | rust |
| `pypdf._protocols` | 24 | 24 | 100% | python |
| `pypdf._reader` | 50 | 50 | 100% | stub |
| `pypdf._text_extraction` | 18 | 18 | 100% | stub |
| `pypdf._text_extraction._layout_mode` | 1 | 1 | 100% | python |
| `pypdf._text_extraction._layout_mode._fixed_width_page` | 19 | 19 | 100% | stub |
| `pypdf._text_extraction._layout_mode._text_state_manager` | 20 | 20 | 100% | stub |
| `pypdf._text_extraction._layout_mode._text_state_params` | 26 | 26 | 100% | stub |
| `pypdf._text_extraction._text_extractor` | 24 | 24 | 100% | stub |
| `pypdf._utils` | 63 | 63 | 100% | rust |
| `pypdf._version` | 1 | 1 | 100% | python |
| `pypdf._writer` | 108 | 131 | 82% | stub |
| `pypdf.actions` | 1 | 1 | 100% | python |
| `pypdf.actions._actions` | 13 | 13 | 100% | rust |
| `pypdf.annotations` | 1 | 1 | 100% | python |
| `pypdf.annotations._base` | 5 | 5 | 100% | rust |
| `pypdf.annotations._markup_annotations` | 20 | 20 | 100% | rust |
| `pypdf.annotations._non_markup_annotations` | 4 | 4 | 100% | rust |
| `pypdf.constants` | 379 | 379 | 100% | python |
| `pypdf.errors` | 15 | 15 | 100% | python |
| `pypdf.filters` | 56 | 56 | 100% | rust |
| `pypdf.generic` | 4 | 4 | 100% | python |
| `pypdf.generic._appearance_stream` | 31 | 31 | 100% | stub |
| `pypdf.generic._base` | 105 | 105 | 100% | rust |
| `pypdf.generic._color` | 22 | 22 | 100% | rust |
| `pypdf.generic._data_structures` | 118 | 118 | 100% | rust |
| `pypdf.generic._files` | 33 | 33 | 100% | stub |
| `pypdf.generic._fit` | 12 | 12 | 100% | rust |
| `pypdf.generic._font` | 61 | 61 | 100% | rust |
| `pypdf.generic._image_inline` | 15 | 15 | 100% | stub |
| `pypdf.generic._image_xobject` | 22 | 22 | 100% | stub |
| `pypdf.generic._link` | 12 | 12 | 100% | rust |
| `pypdf.generic._outline` | 2 | 2 | 100% | rust |
| `pypdf.generic._rectangle` | 23 | 23 | 100% | rust |
| `pypdf.generic._utils` | 11 | 11 | 100% | rust |
| `pypdf.generic._viewerpref` | 14 | 14 | 100% | rust |
| `pypdf.pagerange` | 6 | 15 | 40% | rust |
| `pypdf.papersizes` | 15 | 15 | 100% | python |
| `pypdf.types` | 9 | 9 | 100% | python |
| `pypdf.xmp` | 94 | 94 | 100% | rust |

## 未覆盖符号（前 200 个）

- `pypdf._writer.ALL_DOCUMENT_PERMISSIONS`
- `pypdf._writer.ObjectDeletionFlag`
- `pypdf._writer.ObjectDeletionFlag.NONE`
- `pypdf._writer.ObjectDeletionFlag.TEXT`
- `pypdf._writer.ObjectDeletionFlag.LINKS`
- `pypdf._writer.ObjectDeletionFlag.ATTACHMENTS`
- `pypdf._writer.ObjectDeletionFlag.OBJECTS_3D`
- `pypdf._writer.ObjectDeletionFlag.ALL_ANNOTATIONS`
- `pypdf._writer.ObjectDeletionFlag.XOBJECT_IMAGES`
- `pypdf._writer.ObjectDeletionFlag.INLINE_IMAGES`
- `pypdf._writer.ObjectDeletionFlag.DRAWING_IMAGES`
- `pypdf._writer.ObjectDeletionFlag.IMAGES`
- `pypdf._writer._NAMED_DESTINATION_INDEX_MINIMUM`
- `pypdf._writer._NamedDestinationIndex`
- `pypdf._writer.PdfWriter`
- `pypdf._writer.PdfWriter.incremental`
- `pypdf._writer.PdfWriter._objects`
- `pypdf._writer.PdfWriter._id_translated`
- `pypdf._writer.PdfWriter._reader`
- `pypdf._writer.PdfWriter.FFBITS_NUL`
- `pypdf._writer.PdfWriter._UNSET`
- `pypdf._writer.PdfWriter._valid_layouts`
- `pypdf._writer.PdfWriter._valid_modes`
- `pypdf.pagerange._INT_RE`
- `pypdf.pagerange.PAGE_RANGE_RE`
- `pypdf.pagerange.PageRange`
- `pypdf.pagerange.PageRange.__str__`
- `pypdf.pagerange.PageRange.__repr__`
- `pypdf.pagerange.PageRange.__eq__`
- `pypdf.pagerange.PageRange.__hash__`
- `pypdf.pagerange.PAGE_RANGE_ALL`
- `pypdf.pagerange.PageRangeSpec`
