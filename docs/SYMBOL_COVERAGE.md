# 符号覆盖报告（Symbol Coverage）

> 由 `tools/manifest/check.py` 从 `docs/symbols.json` 与 Rust 源码中的
> `// @pypdf:` / `// @pypdf-module:` 注解自动生成。门禁 A6 要求 100%。

总计：**1466/1934**（75.80%）

| 模块 | 已覆盖 | 总数 | 覆盖 | 实现方式 |
|---|---:|---:|---:|---|
| `pypdf` | 4 | 4 | 100% | python |
| `pypdf._cmap` | 20 | 20 | 100% | stub |
| `pypdf._codecs` | 8 | 8 | 100% | python |
| `pypdf._codecs._codecs` | 18 | 18 | 100% | rust |
| `pypdf._codecs.adobe_glyphs` | 2 | 2 | 100% | python |
| `pypdf._codecs.core_font_metrics` | 0 | 1 | 0% | stub |
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
| `pypdf._doc_common` | 89 | 89 | 100% | stub |
| `pypdf._encryption` | 58 | 58 | 100% | rust |
| `pypdf._page` | 108 | 108 | 100% | stub |
| `pypdf._page_labels` | 28 | 36 | 78% | stub |
| `pypdf._protocols` | 24 | 24 | 100% | python |
| `pypdf._reader` | 0 | 50 | 0% | stub |
| `pypdf._text_extraction` | 0 | 18 | 0% | stub |
| `pypdf._text_extraction._layout_mode` | 1 | 1 | 100% | python |
| `pypdf._text_extraction._layout_mode._fixed_width_page` | 0 | 19 | 0% | stub |
| `pypdf._text_extraction._layout_mode._text_state_manager` | 0 | 20 | 0% | stub |
| `pypdf._text_extraction._layout_mode._text_state_params` | 0 | 26 | 0% | stub |
| `pypdf._text_extraction._text_extractor` | 0 | 24 | 0% | stub |
| `pypdf._utils` | 63 | 63 | 100% | rust |
| `pypdf._version` | 1 | 1 | 100% | python |
| `pypdf._writer` | 0 | 131 | 0% | stub |
| `pypdf.actions` | 1 | 1 | 100% | python |
| `pypdf.actions._actions` | 13 | 13 | 100% | stub |
| `pypdf.annotations` | 1 | 1 | 100% | python |
| `pypdf.annotations._base` | 5 | 5 | 100% | stub |
| `pypdf.annotations._markup_annotations` | 20 | 20 | 100% | stub |
| `pypdf.annotations._non_markup_annotations` | 4 | 4 | 100% | stub |
| `pypdf.constants` | 379 | 379 | 100% | python |
| `pypdf.errors` | 15 | 15 | 100% | python |
| `pypdf.filters` | 56 | 56 | 100% | rust |
| `pypdf.generic` | 4 | 4 | 100% | python |
| `pypdf.generic._appearance_stream` | 0 | 31 | 0% | stub |
| `pypdf.generic._base` | 105 | 105 | 100% | rust |
| `pypdf.generic._color` | 22 | 22 | 100% | rust |
| `pypdf.generic._data_structures` | 118 | 118 | 100% | rust |
| `pypdf.generic._files` | 0 | 33 | 0% | stub |
| `pypdf.generic._fit` | 12 | 12 | 100% | rust |
| `pypdf.generic._font` | 0 | 61 | 0% | stub |
| `pypdf.generic._image_inline` | 0 | 15 | 0% | stub |
| `pypdf.generic._image_xobject` | 0 | 22 | 0% | stub |
| `pypdf.generic._link` | 12 | 12 | 100% | rust |
| `pypdf.generic._outline` | 2 | 2 | 100% | rust |
| `pypdf.generic._rectangle` | 23 | 23 | 100% | rust |
| `pypdf.generic._utils` | 11 | 11 | 100% | rust |
| `pypdf.generic._viewerpref` | 14 | 14 | 100% | rust |
| `pypdf.pagerange` | 6 | 15 | 40% | rust |
| `pypdf.papersizes` | 15 | 15 | 100% | python |
| `pypdf.types` | 9 | 9 | 100% | python |
| `pypdf.xmp` | 94 | 94 | 100% | stub |

## 未覆盖符号（前 200 个）

- `pypdf._codecs.core_font_metrics.CORE_FONT_METRICS`
- `pypdf._page_labels._TreeNode.node`
- `pypdf._page_labels._TreeNode.indexes`
- `pypdf._page_labels._TreeNode.level`
- `pypdf._page_labels._KidPages.kid`
- `pypdf._page_labels._KidPages.indexes`
- `pypdf._page_labels._LeafPages.nums`
- `pypdf._page_labels._LeafPages.leaf`
- `pypdf._page_labels._LeafPages.indexes`
- `pypdf._reader.PdfReader`
- `pypdf._reader.PdfReader.xref`
- `pypdf._reader.PdfReader.trailer`
- `pypdf._reader.PdfReader.__init__`
- `pypdf._reader.PdfReader._initialize_stream`
- `pypdf._reader.PdfReader._handle_encryption`
- `pypdf._reader.PdfReader.__enter__`
- `pypdf._reader.PdfReader.__exit__`
- `pypdf._reader.PdfReader.close`
- `pypdf._reader.PdfReader.root_object`
- `pypdf._reader.PdfReader._info`
- `pypdf._reader.PdfReader._ID`
- `pypdf._reader.PdfReader.pdf_header`
- `pypdf._reader.PdfReader.xmp_metadata`
- `pypdf._reader.PdfReader._get_page_number_by_indirect`
- `pypdf._reader.PdfReader._get_object_from_stream`
- `pypdf._reader.PdfReader.get_object`
- `pypdf._reader.PdfReader.read_object_header`
- `pypdf._reader.PdfReader.cache_get_indirect_object`
- `pypdf._reader.PdfReader.cache_indirect_object`
- `pypdf._reader.PdfReader._replace_object`
- `pypdf._reader.PdfReader.read`
- `pypdf._reader.PdfReader._basic_validation`
- `pypdf._reader.PdfReader._find_eof_marker`
- `pypdf._reader.PdfReader._find_startxref_pos`
- `pypdf._reader.PdfReader._MAX_STARTXREF_RECOVERY_LINES`
- `pypdf._reader.PdfReader._is_valid_startxref_value`
- `pypdf._reader.PdfReader._find_previous_startxref_pos`
- `pypdf._reader.PdfReader._load_recovery_cache`
- `pypdf._reader.PdfReader._read_standard_xref_table__handle_size`
- `pypdf._reader.PdfReader._read_standard_xref_table`
- `pypdf._reader.PdfReader._read_xref_tables_and_trailers`
- `pypdf._reader.PdfReader._XREF_STREAM_TRAILER_KEYS`
- `pypdf._reader.PdfReader._process_xref_stream`
- `pypdf._reader.PdfReader._read_xref`
- `pypdf._reader.PdfReader._read_xref_other_error`
- `pypdf._reader.PdfReader._sanitize_pdf15_xref_stream_index_pairs`
- `pypdf._reader.PdfReader._read_pdf15_xref_stream`
- `pypdf._reader.PdfReader._get_xref_issues`
- `pypdf._reader.PdfReader._find_pdf_objects`
- `pypdf._reader.PdfReader._find_pdf_trailers`
- `pypdf._reader.PdfReader._rebuild_xref_table`
- `pypdf._reader.PdfReader._read_xref_subsections`
- `pypdf._reader.PdfReader._pairs`
- `pypdf._reader.PdfReader.decrypt`
- `pypdf._reader.PdfReader.is_encrypted`
- `pypdf._reader.PdfReader.add_form_topname`
- `pypdf._reader.PdfReader.rename_form_topname`
- `pypdf._reader.PdfReader._repr_mimebundle_`
- `pypdf._reader.PdfReader._get_named_destinations`
- `pypdf._text_extraction.CUSTOM_RTL_MIN`
- `pypdf._text_extraction.CUSTOM_RTL_MAX`
- `pypdf._text_extraction.CUSTOM_RTL_SPECIAL_CHARS`
- `pypdf._text_extraction.LAYOUT_NEW_BT_GROUP_SPACE_WIDTHS`
- `pypdf._text_extraction.UNICODE_LOWER_LIMIT`
- `pypdf._text_extraction.UNICODE_UPPER_LIMIT`
- `pypdf._text_extraction.RTL_CHARACTER_PATTERN`
- `pypdf._text_extraction._TEXT_OPERATORS`
- `pypdf._text_extraction._TEXT_OPERATOR_START`
- `pypdf._text_extraction.OrientationNotFoundError`
- `pypdf._text_extraction.set_custom_rtl`
- `pypdf._text_extraction.mult`
- `pypdf._text_extraction.orient`
- `pypdf._text_extraction.crlf_space_check`
- `pypdf._text_extraction.get_text_operands`
- `pypdf._text_extraction.get_display_str`
- `pypdf._text_extraction._has_text_operator`
- `pypdf._text_extraction._get_content_stream_for_text`
- `pypdf._text_extraction._layout_mode._fixed_width_page.WHITESPACE_LIMIT`
- `pypdf._text_extraction._layout_mode._fixed_width_page.NEWLINE_LIMIT`
- `pypdf._text_extraction._layout_mode._fixed_width_page.BTGroup`
- `pypdf._text_extraction._layout_mode._fixed_width_page.BTGroup.tx`
- `pypdf._text_extraction._layout_mode._fixed_width_page.BTGroup.ty`
- `pypdf._text_extraction._layout_mode._fixed_width_page.BTGroup.font_size`
- `pypdf._text_extraction._layout_mode._fixed_width_page.BTGroup.font_height`
- `pypdf._text_extraction._layout_mode._fixed_width_page.BTGroup.text`
- `pypdf._text_extraction._layout_mode._fixed_width_page.BTGroup.displaced_tx`
- `pypdf._text_extraction._layout_mode._fixed_width_page.BTGroup.flip_sort`
- `pypdf._text_extraction._layout_mode._fixed_width_page.set_font_operation`
- `pypdf._text_extraction._layout_mode._fixed_width_page.resolve_font`
- `pypdf._text_extraction._layout_mode._fixed_width_page.bt_group`
- `pypdf._text_extraction._layout_mode._fixed_width_page._show_text_operand`
- `pypdf._text_extraction._layout_mode._fixed_width_page.recurse_to_target_op`
- `pypdf._text_extraction._layout_mode._fixed_width_page.y_coordinate_groups`
- `pypdf._text_extraction._layout_mode._fixed_width_page.text_show_operations`
- `pypdf._text_extraction._layout_mode._fixed_width_page.fixed_char_width`
- `pypdf._text_extraction._layout_mode._fixed_width_page.fixed_width_page`
- `pypdf._text_extraction._layout_mode._text_state_manager.TextStateManagerKeyType`
- `pypdf._text_extraction._layout_mode._text_state_manager.TextStateManagerChainMapType`
- `pypdf._text_extraction._layout_mode._text_state_manager.TextStateManagerDictType`
- `pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager`
- `pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.__init__`
- `pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.set_state_param`
- `pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.set_font`
- `pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.text_state_params`
- `pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager._as_float`
- `pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.raw_transform`
- `pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.new_transform`
- `pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.reset_tm`
- `pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.reset_trm`
- `pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.remove_q`
- `pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.add_q`
- `pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.add_cm`
- `pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager._complete_matrix`
- `pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.add_tm`
- `pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.add_trm`
- `pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager.effective_transform`
- `pypdf._text_extraction._layout_mode._text_state_params.TextStateParams`
- `pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.value`
- `pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.font`
- `pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.font_size`
- `pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.Tc`
- `pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.Tw`
- `pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.Tz`
- `pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.TL`
- `pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.Ts`
- `pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.transform`
- `pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.tx`
- `pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.ty`
- `pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.displaced_tx`
- `pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.space_tx`
- `pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.font_height`
- `pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.flip_vertical`
- `pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.rotated`
- `pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.text`
- `pypdf._text_extraction._layout_mode._text_state_params.TextStateParams._raw_chars`
- `pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.__post_init__`
- `pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.font_size_matrix`
- `pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.displaced_transform`
- `pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.render_transform`
- `pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.displacement_matrix`
- `pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.word_tx`
- `pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.to_dict`
- `pypdf._text_extraction._text_extractor._get_number_operand`
- `pypdf._text_extraction._text_extractor.TextExtraction`
- `pypdf._text_extraction._text_extractor.TextExtraction.__init__`
- `pypdf._text_extraction._text_extractor.TextExtraction.initialize_extraction`
- `pypdf._text_extraction._text_extractor.TextExtraction.compute_str_widths`
- `pypdf._text_extraction._text_extractor.TextExtraction.process_operation`
- `pypdf._text_extraction._text_extractor.TextExtraction.process_tj_offset`
- `pypdf._text_extraction._text_extractor.TextExtraction._post_process_text_operation`
- `pypdf._text_extraction._text_extractor.TextExtraction._handle_tj`
- `pypdf._text_extraction._text_extractor.TextExtraction._flush_text`
- `pypdf._text_extraction._text_extractor.TextExtraction._handle_bt`
- `pypdf._text_extraction._text_extractor.TextExtraction._handle_et`
- `pypdf._text_extraction._text_extractor.TextExtraction._handle_save_graphics_state`
- `pypdf._text_extraction._text_extractor.TextExtraction._handle_restore_graphics_state`
- `pypdf._text_extraction._text_extractor.TextExtraction._handle_cm`
- `pypdf._text_extraction._text_extractor.TextExtraction._handle_tz`
- `pypdf._text_extraction._text_extractor.TextExtraction._handle_tc`
- `pypdf._text_extraction._text_extractor.TextExtraction._handle_tw`
- `pypdf._text_extraction._text_extractor.TextExtraction._handle_tl`
- `pypdf._text_extraction._text_extractor.TextExtraction._handle_tf`
- `pypdf._text_extraction._text_extractor.TextExtraction._handle_td`
- `pypdf._text_extraction._text_extractor.TextExtraction._handle_tm`
- `pypdf._text_extraction._text_extractor.TextExtraction._handle_t_star`
- `pypdf._text_extraction._text_extractor.TextExtraction._handle_tj_operation`
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
- `pypdf._writer._NamedDestinationIndex.__init__`
- `pypdf._writer._NamedDestinationIndex._update`
- `pypdf._writer._NamedDestinationIndex.contains`
- `pypdf._writer._NamedDestinationIndex.insert`
- `pypdf._writer._NamedDestinationIndex.record_insertion`
- `pypdf._writer._NamedDestinationIndex.flush`
- `pypdf._writer.PdfWriter`
- `pypdf._writer.PdfWriter.incremental`
- `pypdf._writer.PdfWriter._objects`
- `pypdf._writer.PdfWriter._id_translated`
- `pypdf._writer.PdfWriter._reader`
- `pypdf._writer.PdfWriter.__init__`
- `pypdf._writer.PdfWriter.is_encrypted`
- `pypdf._writer.PdfWriter.root_object`
- `pypdf._writer.PdfWriter._info`
- `pypdf._writer.PdfWriter._info`
- `pypdf._writer.PdfWriter.xmp_metadata`
- `pypdf._writer.PdfWriter.xmp_metadata`
- `pypdf._writer.PdfWriter.__enter__`
- `pypdf._writer.PdfWriter.__exit__`
