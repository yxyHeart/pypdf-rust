# 符号覆盖报告（Symbol Coverage）

> 由 `tools/manifest/check.py` 从 `docs/symbols.json` 与 Rust 源码中的
> `// @pypdf:` / `// @pypdf-module:` 注解自动生成。门禁 A6 要求 100%。

总计：**886/1934**（45.81%）

| 模块 | 已覆盖 | 总数 | 覆盖 | 实现方式 |
|---|---:|---:|---:|---|
| `pypdf` | 4 | 4 | 100% | python |
| `pypdf._cmap` | 0 | 20 | 0% | stub |
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
| `pypdf._text_extraction._layout_mode` | 1 | 1 | 100% | python |
| `pypdf._text_extraction._layout_mode._fixed_width_page` | 0 | 19 | 0% | stub |
| `pypdf._text_extraction._layout_mode._text_state_manager` | 0 | 20 | 0% | stub |
| `pypdf._text_extraction._layout_mode._text_state_params` | 0 | 26 | 0% | stub |
| `pypdf._text_extraction._text_extractor` | 0 | 24 | 0% | stub |
| `pypdf._utils` | 63 | 63 | 100% | rust |
| `pypdf._version` | 1 | 1 | 100% | python |
| `pypdf._writer` | 0 | 131 | 0% | stub |
| `pypdf.actions` | 1 | 1 | 100% | python |
| `pypdf.actions._actions` | 0 | 13 | 0% | stub |
| `pypdf.annotations` | 1 | 1 | 100% | python |
| `pypdf.annotations._base` | 0 | 5 | 0% | stub |
| `pypdf.annotations._markup_annotations` | 0 | 20 | 0% | stub |
| `pypdf.annotations._non_markup_annotations` | 0 | 4 | 0% | stub |
| `pypdf.constants` | 379 | 379 | 100% | python |
| `pypdf.errors` | 15 | 15 | 100% | python |
| `pypdf.filters` | 56 | 56 | 100% | stub |
| `pypdf.generic` | 4 | 4 | 100% | python |
| `pypdf.generic._appearance_stream` | 0 | 31 | 0% | stub |
| `pypdf.generic._base` | 105 | 105 | 100% | rust |
| `pypdf.generic._color` | 0 | 22 | 0% | stub |
| `pypdf.generic._data_structures` | 115 | 118 | 97% | stub |
| `pypdf.generic._files` | 0 | 33 | 0% | stub |
| `pypdf.generic._fit` | 0 | 12 | 0% | stub |
| `pypdf.generic._font` | 0 | 61 | 0% | stub |
| `pypdf.generic._image_inline` | 0 | 15 | 0% | stub |
| `pypdf.generic._image_xobject` | 0 | 22 | 0% | stub |
| `pypdf.generic._link` | 0 | 12 | 0% | stub |
| `pypdf.generic._outline` | 0 | 2 | 0% | stub |
| `pypdf.generic._rectangle` | 0 | 23 | 0% | stub |
| `pypdf.generic._utils` | 11 | 11 | 100% | rust |
| `pypdf.generic._viewerpref` | 0 | 14 | 0% | stub |
| `pypdf.pagerange` | 6 | 15 | 40% | rust |
| `pypdf.papersizes` | 15 | 15 | 100% | python |
| `pypdf.types` | 9 | 9 | 100% | python |
| `pypdf.xmp` | 0 | 94 | 0% | stub |

## 未覆盖符号（前 200 个）

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
- `pypdf._codecs.core_font_metrics.CORE_FONT_METRICS`
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
- `pypdf._doc_common.PdfDocCommon._get_named_destinations`
- `pypdf._doc_common.PdfDocCommon.get_fields`
- `pypdf._doc_common.PdfDocCommon._get_qualified_field_name`
- `pypdf._doc_common.PdfDocCommon._normal_appearance`
- `pypdf._doc_common.PdfDocCommon._build_field`
- `pypdf._doc_common.PdfDocCommon._check_kids`
- `pypdf._doc_common.PdfDocCommon._write_field`
- `pypdf._doc_common.PdfDocCommon.get_form_text_fields`
- `pypdf._doc_common.PdfDocCommon.get_pages_showing_field`
- `pypdf._doc_common.PdfDocCommon.open_destination`
- `pypdf._doc_common.PdfDocCommon.open_destination`
- `pypdf._doc_common.PdfDocCommon.outline`
- `pypdf._doc_common.PdfDocCommon._get_outline`
- `pypdf._doc_common.PdfDocCommon.threads`
- `pypdf._doc_common.PdfDocCommon._get_page_number_by_indirect`
- `pypdf._doc_common.PdfDocCommon.get_page_number`
- `pypdf._doc_common.PdfDocCommon.get_destination_page_number`
- `pypdf._doc_common.PdfDocCommon._build_destination`
- `pypdf._doc_common.PdfDocCommon._build_outline_item`
- `pypdf._doc_common.PdfDocCommon.pages`
- `pypdf._doc_common.PdfDocCommon.page_labels`
- `pypdf._doc_common.PdfDocCommon.page_layout`
- `pypdf._doc_common.PdfDocCommon.page_mode`
- `pypdf._doc_common.PdfDocCommon._flatten`
- `pypdf._doc_common.PdfDocCommon._enter_page_tree_node`
- `pypdf._doc_common.PdfDocCommon._pop_next_page_tree_kid`
- `pypdf._doc_common.PdfDocCommon._flatten_leaf_page`
- `pypdf._doc_common.PdfDocCommon.remove_page`
- `pypdf._doc_common.PdfDocCommon._get_indirect_object`
- `pypdf._doc_common.PdfDocCommon.user_access_permissions`
- `pypdf._doc_common.PdfDocCommon.are_permissions_valid`
- `pypdf._doc_common.PdfDocCommon.is_encrypted`
- `pypdf._doc_common.PdfDocCommon.xfa`
- `pypdf._doc_common.PdfDocCommon.attachments`
- `pypdf._doc_common.PdfDocCommon._get_attachment_contents`
- `pypdf._doc_common.PdfDocCommon.attachment_list`
- `pypdf._doc_common.PdfDocCommon._repr_mimebundle_`
- `pypdf._doc_common.LazyDict`
- `pypdf._doc_common.LazyDict.__init__`
- `pypdf._doc_common.LazyDict.__getitem__`
- `pypdf._doc_common.LazyDict.__iter__`
- `pypdf._doc_common.LazyDict.__len__`
- `pypdf._doc_common.LazyDict.__str__`
- `pypdf._encryption.CryptFilter`
- `pypdf._encryption.CryptFilter.__init__`
- `pypdf._encryption.CryptFilter.encrypt_object`
- `pypdf._encryption.CryptFilter.decrypt_object`
- `pypdf._encryption._PADDING`
- `pypdf._encryption._padding`
- `pypdf._encryption._SASLPREP_PROHIBITED_CHECKS`
- `pypdf._encryption.AlgV4`
- `pypdf._encryption.AlgV4.compute_key`
- `pypdf._encryption.AlgV4.compute_O_value_key`
- `pypdf._encryption.AlgV4.compute_O_value`
- `pypdf._encryption.AlgV4.compute_U_value`
- `pypdf._encryption.AlgV4.verify_user_password`
- `pypdf._encryption.AlgV4.verify_owner_password`
- `pypdf._encryption.AlgV5`
- `pypdf._encryption.AlgV5.verify_owner_password`
- `pypdf._encryption.AlgV5.verify_user_password`
- `pypdf._encryption.AlgV5.calculate_hash`
- `pypdf._encryption.AlgV5.verify_perms`
- `pypdf._encryption.AlgV5.generate_values`
- `pypdf._encryption.AlgV5.compute_U_value`
- `pypdf._encryption.AlgV5.compute_O_value`
- `pypdf._encryption.AlgV5.compute_Perms_value`
- `pypdf._encryption.PasswordType`
- `pypdf._encryption.PasswordType.NOT_DECRYPTED`
- `pypdf._encryption.PasswordType.USER_PASSWORD`
- `pypdf._encryption.PasswordType.OWNER_PASSWORD`
- `pypdf._encryption._saslprep`
- `pypdf._encryption.EncryptAlgorithm`
- `pypdf._encryption.EncryptAlgorithm.RC4_40`
- `pypdf._encryption.EncryptAlgorithm.RC4_128`
- `pypdf._encryption.EncryptAlgorithm.AES_128`
- `pypdf._encryption.EncryptAlgorithm.AES_256_R5`
- `pypdf._encryption.EncryptAlgorithm.AES_256`
- `pypdf._encryption.EncryptionValues`
- `pypdf._encryption.EncryptionValues.O`
- `pypdf._encryption.EncryptionValues.U`
