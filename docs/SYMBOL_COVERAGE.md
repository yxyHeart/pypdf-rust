# 符号覆盖报告（Symbol Coverage）

> 由 `tools/manifest/check.py` 从 `docs/symbols.json` 与 Rust 源码中的
> `// @pypdf:` / `// @pypdf-module:` 注解自动生成。门禁 A6 要求 100%。

总计：**49/1934**（2.53%）

| 模块 | 已覆盖 | 总数 | 覆盖 | 实现方式 |
|---|---:|---:|---:|---|
| `pypdf` | 0 | 4 | 0% | stub |
| `pypdf._cmap` | 0 | 20 | 0% | stub |
| `pypdf._codecs` | 0 | 8 | 0% | stub |
| `pypdf._codecs._codecs` | 0 | 18 | 0% | stub |
| `pypdf._codecs.adobe_glyphs` | 0 | 2 | 0% | stub |
| `pypdf._codecs.core_font_metrics` | 0 | 1 | 0% | stub |
| `pypdf._codecs.pdfdoc` | 0 | 1 | 0% | stub |
| `pypdf._codecs.std` | 0 | 1 | 0% | stub |
| `pypdf._codecs.symbol` | 0 | 1 | 0% | stub |
| `pypdf._codecs.zapfding` | 0 | 1 | 0% | stub |
| `pypdf._configuration` | 0 | 43 | 0% | stub |
| `pypdf._crypt_providers` | 0 | 1 | 0% | stub |
| `pypdf._crypt_providers._base` | 0 | 4 | 0% | stub |
| `pypdf._crypt_providers._cryptography` | 0 | 18 | 0% | stub |
| `pypdf._crypt_providers._fallback` | 0 | 16 | 0% | stub |
| `pypdf._crypt_providers._pycryptodome` | 0 | 15 | 0% | stub |
| `pypdf._doc_common` | 0 | 89 | 0% | stub |
| `pypdf._encryption` | 0 | 58 | 0% | stub |
| `pypdf._page` | 0 | 108 | 0% | stub |
| `pypdf._page_labels` | 0 | 36 | 0% | stub |
| `pypdf._protocols` | 24 | 24 | 100% | python |
| `pypdf._reader` | 0 | 50 | 0% | stub |
| `pypdf._text_extraction` | 0 | 18 | 0% | stub |
| `pypdf._text_extraction._layout_mode` | 0 | 1 | 0% | stub |
| `pypdf._text_extraction._layout_mode._fixed_width_page` | 0 | 19 | 0% | stub |
| `pypdf._text_extraction._layout_mode._text_state_manager` | 0 | 20 | 0% | stub |
| `pypdf._text_extraction._layout_mode._text_state_params` | 0 | 26 | 0% | stub |
| `pypdf._text_extraction._text_extractor` | 0 | 24 | 0% | stub |
| `pypdf._utils` | 0 | 63 | 0% | stub |
| `pypdf._version` | 1 | 1 | 100% | python |
| `pypdf._writer` | 0 | 131 | 0% | stub |
| `pypdf.actions` | 0 | 1 | 0% | stub |
| `pypdf.actions._actions` | 0 | 13 | 0% | stub |
| `pypdf.annotations` | 0 | 1 | 0% | stub |
| `pypdf.annotations._base` | 0 | 5 | 0% | stub |
| `pypdf.annotations._markup_annotations` | 0 | 20 | 0% | stub |
| `pypdf.annotations._non_markup_annotations` | 0 | 4 | 0% | stub |
| `pypdf.constants` | 0 | 379 | 0% | stub |
| `pypdf.errors` | 15 | 15 | 100% | python |
| `pypdf.filters` | 0 | 56 | 0% | stub |
| `pypdf.generic` | 0 | 4 | 0% | stub |
| `pypdf.generic._appearance_stream` | 0 | 31 | 0% | stub |
| `pypdf.generic._base` | 0 | 105 | 0% | stub |
| `pypdf.generic._color` | 0 | 22 | 0% | stub |
| `pypdf.generic._data_structures` | 0 | 118 | 0% | stub |
| `pypdf.generic._files` | 0 | 33 | 0% | stub |
| `pypdf.generic._fit` | 0 | 12 | 0% | stub |
| `pypdf.generic._font` | 0 | 61 | 0% | stub |
| `pypdf.generic._image_inline` | 0 | 15 | 0% | stub |
| `pypdf.generic._image_xobject` | 0 | 22 | 0% | stub |
| `pypdf.generic._link` | 0 | 12 | 0% | stub |
| `pypdf.generic._outline` | 0 | 2 | 0% | stub |
| `pypdf.generic._rectangle` | 0 | 23 | 0% | stub |
| `pypdf.generic._utils` | 0 | 11 | 0% | stub |
| `pypdf.generic._viewerpref` | 0 | 14 | 0% | stub |
| `pypdf.pagerange` | 0 | 15 | 0% | stub |
| `pypdf.papersizes` | 0 | 15 | 0% | stub |
| `pypdf.types` | 9 | 9 | 100% | python |
| `pypdf.xmp` | 0 | 94 | 0% | stub |

## 未覆盖符号（前 200 个）

- `pypdf.pil_version`
- `pypdf.pil_version`
- `pypdf._debug_versions`
- `pypdf.__all__`
- `pypdf._cmap._predefined_cmap`
- `pypdf._cmap.get_encoding`
- `pypdf._cmap._parse_encoding`
- `pypdf._cmap._parse_to_unicode`
- `pypdf._cmap.prepare_cm`
- `pypdf._cmap.process_cm_line`
- `pypdf._cmap.MAPPING_DICTIONARY_SIZE_LIMIT`
- `pypdf._cmap.MAX_CMAP_CODE_BYTES`
- `pypdf._cmap.MAX_CMAP_STRING_BYTES`
- `pypdf._cmap.MAX_CMAP_CODE_BYTES_LIMIT`
- `pypdf._cmap.MAX_CMAP_STRING_BYTES_LIMIT`
- `pypdf._cmap._check_mapping_size`
- `pypdf._cmap._check_token_length`
- `pypdf._cmap.__parse_bfrange__decode`
- `pypdf._cmap.parse_bfrange`
- `pypdf._cmap.parse_bfchar`
- `pypdf._cmap._glyph_name_to_unicode`
- `pypdf._cmap._derive_character_map_from_font_file`
- `pypdf._cmap._derive_character_map_from_cff_type1_font_file`
- `pypdf._cmap._derive_character_map_from_type1_font_file`
- `pypdf._codecs.fill_from_encoding`
- `pypdf._codecs.rev_encoding`
- `pypdf._codecs.encoding_dict_from_named_encoding`
- `pypdf._codecs._win_encoding`
- `pypdf._codecs._mac_encoding`
- `pypdf._codecs._pdfdoc_encoding_rev`
- `pypdf._codecs.charset_encoding`
- `pypdf._codecs.__all__`
- `pypdf._codecs._codecs.Codec`
- `pypdf._codecs._codecs.Codec.encode`
- `pypdf._codecs._codecs.Codec.decode`
- `pypdf._codecs._codecs.LzwCodec`
- `pypdf._codecs._codecs.LzwCodec.CLEAR_TABLE_MARKER`
- `pypdf._codecs._codecs.LzwCodec.EOD_MARKER`
- `pypdf._codecs._codecs.LzwCodec.INITIAL_BITS_PER_CODE`
- `pypdf._codecs._codecs.LzwCodec.MAX_BITS_PER_CODE`
- `pypdf._codecs._codecs.LzwCodec._AND_TABLE`
- `pypdf._codecs._codecs.LzwCodec.__init__`
- `pypdf._codecs._codecs.LzwCodec._initialize_encoding_table`
- `pypdf._codecs._codecs.LzwCodec._increase_next_code`
- `pypdf._codecs._codecs.LzwCodec.encode`
- `pypdf._codecs._codecs.LzwCodec._pack_codes_into_bytes`
- `pypdf._codecs._codecs.LzwCodec._initialize_decoding_table`
- `pypdf._codecs._codecs.LzwCodec._next_code_decode`
- `pypdf._codecs._codecs.LzwCodec.decode`
- `pypdf._codecs._codecs.LzwCodec._add_entry_decode`
- `pypdf._codecs.adobe_glyphs.adobe_glyphs`
- `pypdf._codecs.adobe_glyphs._complete`
- `pypdf._codecs.core_font_metrics.CORE_FONT_METRICS`
- `pypdf._codecs.pdfdoc._pdfdoc_encoding`
- `pypdf._codecs.std._std_encoding`
- `pypdf._codecs.symbol._symbol_encoding`
- `pypdf._codecs.zapfding._zapfding_encoding`
- `pypdf._configuration._determine_jbig2dec_binary`
- `pypdf._configuration.Configuration`
- `pypdf._configuration.Configuration.maximum_declared_stream_length`
- `pypdf._configuration.Configuration.array_based_stream_maximum_output_length`
- `pypdf._configuration.Configuration.array_maximum_element_count`
- `pypdf._configuration.Configuration.dictionary_maximum_element_count`
- `pypdf._configuration.Configuration.jbig2_maximum_output_length`
- `pypdf._configuration.Configuration.lzw_maximum_output_length`
- `pypdf._configuration.Configuration.run_length_maximum_output_length`
- `pypdf._configuration.Configuration.zlib_maximum_output_length`
- `pypdf._configuration.Configuration.zlib_maximum_recovery_input_length`
- `pypdf._configuration.Configuration.brotli_maximum_output_length`
- `pypdf._configuration.Configuration.flate_maximum_columns`
- `pypdf._configuration.Configuration.flate_maximum_row_length`
- `pypdf._configuration.Configuration.image_maximum_buffer_size`
- `pypdf._configuration.Configuration.stream_filters_maximum_length`
- `pypdf._configuration.Configuration.stream_decoding_work_maximum_length`
- `pypdf._configuration.Configuration.stream_parsing_work_maximum_count`
- `pypdf._configuration.Configuration.xmp_maximum_input_length`
- `pypdf._configuration.Configuration.xmp_maximum_element_count`
- `pypdf._configuration.Configuration.outline_maximum_entries`
- `pypdf._configuration.Configuration.outline_maximum_depth`
- `pypdf._configuration.Configuration.page_tree_maximum_entries`
- `pypdf._configuration.Configuration.page_tree_maximum_depth`
- `pypdf._configuration.Configuration.page_tree_maximum_page_entries`
- `pypdf._configuration.Configuration.form_fields_maximum_entries`
- `pypdf._configuration.Configuration.form_fields_maximum_depth`
- `pypdf._configuration.Configuration.named_destinations_maximum_entries`
- `pypdf._configuration.Configuration.named_destinations_maximum_depth`
- `pypdf._configuration.Configuration.xform_maximum_invocations_per_extraction`
- `pypdf._configuration.Configuration.xfa_decoding_work_maximum_length`
- `pypdf._configuration.Configuration.jbig2dec_binary`
- `pypdf._configuration.Configuration.page_merge_box`
- `pypdf._configuration.Configuration.disable_legacy_handling`
- `pypdf._configuration.Configuration.with_overwrites`
- `pypdf._configuration.DEFAULT_CONFIGURATION`
- `pypdf._configuration.CURRENT_CONFIGURATION`
- `pypdf._configuration.get_configuration`
- `pypdf._configuration._build_configuration`
- `pypdf._configuration.overwrite_configuration`
- `pypdf._configuration.apply_configuration`
- `pypdf._configuration.LEGACY_NAME_MAPPING`
- `pypdf._configuration.apply_legacy_configuration`
- `pypdf._crypt_providers.__all__`
- `pypdf._crypt_providers._base.CryptBase`
- `pypdf._crypt_providers._base.CryptBase.encrypt`
- `pypdf._crypt_providers._base.CryptBase.decrypt`
- `pypdf._crypt_providers._base.CryptIdentity`
- `pypdf._crypt_providers._cryptography.crypt_provider`
- `pypdf._crypt_providers._cryptography.CryptRC4`
- `pypdf._crypt_providers._cryptography.CryptRC4._is_rc4_supported`
- `pypdf._crypt_providers._cryptography.CryptRC4.__init__`
- `pypdf._crypt_providers._cryptography.CryptRC4._pure_python_rc4`
- `pypdf._crypt_providers._cryptography.CryptRC4._disable_rc4`
- `pypdf._crypt_providers._cryptography.CryptRC4.encrypt`
- `pypdf._crypt_providers._cryptography.CryptRC4.decrypt`
- `pypdf._crypt_providers._cryptography.CryptAES`
- `pypdf._crypt_providers._cryptography.CryptAES.__init__`
- `pypdf._crypt_providers._cryptography.CryptAES.encrypt`
- `pypdf._crypt_providers._cryptography.CryptAES.decrypt`
- `pypdf._crypt_providers._cryptography.rc4_encrypt`
- `pypdf._crypt_providers._cryptography.rc4_decrypt`
- `pypdf._crypt_providers._cryptography.aes_ecb_encrypt`
- `pypdf._crypt_providers._cryptography.aes_ecb_decrypt`
- `pypdf._crypt_providers._cryptography.aes_cbc_encrypt`
- `pypdf._crypt_providers._cryptography.aes_cbc_decrypt`
- `pypdf._crypt_providers._fallback._DEPENDENCY_ERROR_STR`
- `pypdf._crypt_providers._fallback.crypt_provider`
- `pypdf._crypt_providers._fallback.CryptRC4`
- `pypdf._crypt_providers._fallback.CryptRC4.__init__`
- `pypdf._crypt_providers._fallback.CryptRC4.encrypt`
- `pypdf._crypt_providers._fallback.CryptRC4.decrypt`
- `pypdf._crypt_providers._fallback.CryptAES`
- `pypdf._crypt_providers._fallback.CryptAES.__init__`
- `pypdf._crypt_providers._fallback.CryptAES.encrypt`
- `pypdf._crypt_providers._fallback.CryptAES.decrypt`
- `pypdf._crypt_providers._fallback.rc4_encrypt`
- `pypdf._crypt_providers._fallback.rc4_decrypt`
- `pypdf._crypt_providers._fallback.aes_ecb_encrypt`
- `pypdf._crypt_providers._fallback.aes_ecb_decrypt`
- `pypdf._crypt_providers._fallback.aes_cbc_encrypt`
- `pypdf._crypt_providers._fallback.aes_cbc_decrypt`
- `pypdf._crypt_providers._pycryptodome.crypt_provider`
- `pypdf._crypt_providers._pycryptodome.CryptRC4`
- `pypdf._crypt_providers._pycryptodome.CryptRC4.__init__`
- `pypdf._crypt_providers._pycryptodome.CryptRC4.encrypt`
- `pypdf._crypt_providers._pycryptodome.CryptRC4.decrypt`
- `pypdf._crypt_providers._pycryptodome.CryptAES`
- `pypdf._crypt_providers._pycryptodome.CryptAES.__init__`
- `pypdf._crypt_providers._pycryptodome.CryptAES.encrypt`
- `pypdf._crypt_providers._pycryptodome.CryptAES.decrypt`
- `pypdf._crypt_providers._pycryptodome.rc4_encrypt`
- `pypdf._crypt_providers._pycryptodome.rc4_decrypt`
- `pypdf._crypt_providers._pycryptodome.aes_ecb_encrypt`
- `pypdf._crypt_providers._pycryptodome.aes_ecb_decrypt`
- `pypdf._crypt_providers._pycryptodome.aes_cbc_encrypt`
- `pypdf._crypt_providers._pycryptodome.aes_cbc_decrypt`
- `pypdf._doc_common.convert_to_int`
- `pypdf._doc_common._INHERITABLE_PAGE_ATTRIBUTES`
- `pypdf._doc_common._PageTreeItem`
- `pypdf._doc_common._PageTreeItem.node`
- `pypdf._doc_common._PageTreeItem.inherit`
- `pypdf._doc_common._PageTreeItem.indirect_reference`
- `pypdf._doc_common._PageTreeItem.depth`
- `pypdf._doc_common._PageTreeFrame`
- `pypdf._doc_common._UNSET`
- `pypdf._doc_common.DocumentInformation`
- `pypdf._doc_common.DocumentInformation.__init__`
- `pypdf._doc_common.DocumentInformation._get_text`
- `pypdf._doc_common.DocumentInformation.title`
- `pypdf._doc_common.DocumentInformation.title_raw`
- `pypdf._doc_common.DocumentInformation.author`
- `pypdf._doc_common.DocumentInformation.author_raw`
- `pypdf._doc_common.DocumentInformation.subject`
- `pypdf._doc_common.DocumentInformation.subject_raw`
- `pypdf._doc_common.DocumentInformation.creator`
- `pypdf._doc_common.DocumentInformation.creator_raw`
- `pypdf._doc_common.DocumentInformation.producer`
- `pypdf._doc_common.DocumentInformation.producer_raw`
- `pypdf._doc_common.DocumentInformation.creation_date`
- `pypdf._doc_common.DocumentInformation.creation_date_raw`
- `pypdf._doc_common.DocumentInformation.modification_date`
- `pypdf._doc_common.DocumentInformation.modification_date_raw`
- `pypdf._doc_common.DocumentInformation.keywords`
- `pypdf._doc_common.DocumentInformation.keywords_raw`
- `pypdf._doc_common.PdfDocCommon`
- `pypdf._doc_common.PdfDocCommon.strict`
- `pypdf._doc_common.PdfDocCommon.flattened_pages`
- `pypdf._doc_common.PdfDocCommon._encryption`
- `pypdf._doc_common.PdfDocCommon._readonly`
- `pypdf._doc_common.PdfDocCommon.root_object`
- `pypdf._doc_common.PdfDocCommon.pdf_header`
- `pypdf._doc_common.PdfDocCommon.get_object`
- `pypdf._doc_common.PdfDocCommon._replace_object`
- `pypdf._doc_common.PdfDocCommon._info`
- `pypdf._doc_common.PdfDocCommon.metadata`
- `pypdf._doc_common.PdfDocCommon.xmp_metadata`
- `pypdf._doc_common.PdfDocCommon.viewer_preferences`
- `pypdf._doc_common.PdfDocCommon.get_num_pages`
- `pypdf._doc_common.PdfDocCommon.get_page`
- `pypdf._doc_common.PdfDocCommon._get_page_in_node`
- `pypdf._doc_common.PdfDocCommon.named_destinations`
- `pypdf._doc_common.PdfDocCommon.get_named_dest_root`
