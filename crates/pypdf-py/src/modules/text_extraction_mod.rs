//! Rust port of `pypdf/_text_extraction` (upstream pypdf 7.1.1).
//!
//! # Design: Python class model, Rust logic
//!
//! The five upstream modules are shimmed in `python/pypdf/_text_extraction/`:
//! classes (`TextExtraction`, `TextStateManager`, the `TextStateParams`
//! dataclass, the `BTGroup` TypedDict, `OrientationNotFoundError`), module
//! constants (including the mutable `CUSTOM_RTL_*` globals) and the
//! `ChainMap`/`Counter` state stay Python objects exactly like upstream, because
//! they are observable Python API (tests mutate the globals through
//! `set_custom_rtl`, `monkeypatch.setattr` the `is_char_*` helpers, subclass and
//! instantiate the classes, and compare `dataclasses.fields`).  Every function
//! and method body is a one-line delegation to a `#[pyfunction]` implemented in
//! this module tree.
//!
//! Submodules:
//! * [`init_mod`] — `pypdf/_text_extraction/__init__.py`
//!   (`mult`, `orient`, `crlf_space_check`, `get_text_operands`,
//!   `get_display_str`, `set_custom_rtl`, `_has_text_operator`,
//!   `_get_content_stream_for_text`);
//! * [`extractor`] — `_text_extractor.py` (`TextExtraction`);
//! * [`layout_page`] — `_layout_mode/_fixed_width_page.py`;
//! * [`layout_state`] — `_layout_mode/_text_state_manager.py` and
//!   `_layout_mode/_text_state_params.py`;
//! * [`common`] — helpers wrapping Python semantics;
//! * [`callables`] — tiny Python callables replacing trivial are still Python lambdas.
//!
//! Fidelity notes (hard-won): all string payloads stay Python objects because
//! `surrogatepass` decoding can produce lone surrogates, which a Rust `String`
//! cannot hold; numeric results go through Python operators (`int`/`float`
//! distinctions are observable); module globals such as `CUSTOM_RTL_MIN` and
//! `is_char_neutral` are resolved through the shim module at call time so
//! `set_custom_rtl()` and monkeypatching behave exactly like upstream.

#[path = "text_extraction_mod/callables.rs"]
mod callables;
#[path = "text_extraction_mod/common.rs"]
pub mod common;
#[path = "text_extraction_mod/extractor.rs"]
mod extractor;
#[path = "text_extraction_mod/init_mod.rs"]
mod init_mod;
#[path = "text_extraction_mod/layout_page.rs"]
mod layout_page;
#[path = "text_extraction_mod/layout_state.rs"]
mod layout_state;

pub fn register(module: &pyo3::Bound<'_, pyo3::types::PyModule>) -> pyo3::PyResult<()> {
    use pyo3::prelude::*;

    // pypdf._text_extraction
    module.add_function(wrap_pyfunction!(
        init_mod::text_extraction_set_custom_rtl,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(init_mod::text_extraction_mult, module)?)?;
    module.add_function(wrap_pyfunction!(init_mod::text_extraction_orient, module)?)?;
    module.add_function(wrap_pyfunction!(
        init_mod::text_extraction_crlf_space_check,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        init_mod::text_extraction_get_text_operands,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        init_mod::text_extraction_get_display_str,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        init_mod::text_extraction_has_text_operator,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        init_mod::text_extraction_get_content_stream_for_text,
        module
    )?)?;

    // pypdf._text_extraction._text_extractor
    module.add_function(wrap_pyfunction!(
        extractor::text_extractor_get_number_operand,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(extractor::text_extractor_init, module)?)?;
    module.add_function(wrap_pyfunction!(
        extractor::text_extractor_initialize_extraction,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        extractor::text_extractor_compute_str_widths,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        extractor::text_extractor_process_operation,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        extractor::text_extractor_process_tj_offset,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        extractor::text_extractor_post_process_text_operation,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        extractor::text_extractor_handle_tj,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        extractor::text_extractor_flush_text,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        extractor::text_extractor_handle_bt,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        extractor::text_extractor_handle_et,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        extractor::text_extractor_handle_save_graphics_state,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        extractor::text_extractor_handle_restore_graphics_state,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        extractor::text_extractor_handle_cm,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        extractor::text_extractor_handle_tz,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        extractor::text_extractor_handle_tc,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        extractor::text_extractor_handle_tw,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        extractor::text_extractor_handle_tl,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        extractor::text_extractor_handle_tf,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        extractor::text_extractor_handle_td,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        extractor::text_extractor_handle_tm,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        extractor::text_extractor_handle_t_star,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        extractor::text_extractor_handle_tj_operation,
        module
    )?)?;

    // pypdf._text_extraction._layout_mode._fixed_width_page
    module.add_function(wrap_pyfunction!(
        layout_page::layout_set_font_operation,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(layout_page::layout_resolve_font, module)?)?;
    module.add_function(wrap_pyfunction!(layout_page::layout_bt_group, module)?)?;
    module.add_function(wrap_pyfunction!(
        layout_page::layout_show_text_operand,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        layout_page::layout_recurse_to_target_op,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        layout_page::layout_y_coordinate_groups,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        layout_page::layout_text_show_operations,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        layout_page::layout_fixed_char_width,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        layout_page::layout_fixed_width_page,
        module
    )?)?;

    // pypdf._text_extraction._layout_mode._text_state_manager
    module.add_function(wrap_pyfunction!(
        layout_state::text_state_manager_init,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        layout_state::text_state_manager_set_state_param,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        layout_state::text_state_manager_set_font,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        layout_state::text_state_manager_text_state_params,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        layout_state::text_state_manager_as_float,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        layout_state::text_state_manager_raw_transform,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        layout_state::text_state_manager_new_transform,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        layout_state::text_state_manager_reset_tm,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        layout_state::text_state_manager_reset_trm,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        layout_state::text_state_manager_remove_q,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        layout_state::text_state_manager_add_q,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        layout_state::text_state_manager_add_cm,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        layout_state::text_state_manager_complete_matrix,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        layout_state::text_state_manager_add_tm,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        layout_state::text_state_manager_add_trm,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        layout_state::text_state_manager_effective_transform,
        module
    )?)?;

    // pypdf._text_extraction._layout_mode._text_state_params
    module.add_function(wrap_pyfunction!(
        layout_state::text_state_params_post_init,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        layout_state::text_state_params_font_size_matrix,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        layout_state::text_state_params_displaced_transform,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        layout_state::text_state_params_render_transform,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        layout_state::text_state_params_displacement_matrix,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        layout_state::text_state_params_word_tx,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(
        layout_state::text_state_params_to_dict,
        module
    )?)?;

    // Internal Python-callable stand-ins for upstream lambdas.
    module.add_class::<callables::GroupKey>()?;
    module.add_class::<callables::SortByTx>()?;
    module.add_class::<callables::SortKey>()?;
    module.add_class::<callables::TjDefault>()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Symbols implemented in the hand-written shims
// (`python/pypdf/_text_extraction/**`).
//
// Classes, dataclass/TypedDict definitions, module constants and the mutable
// `CUSTOM_RTL_*` globals are *definitions*: they must exist on the Python side so
// that `isinstance`, subclassing, `dataclasses.fields`, `TypedDict` semantics and
// module-global mutation match upstream exactly.  Each is listed here for gate
// A6; the implementation of every function/method body lives in this module tree.
//
// @pypdf: pypdf._text_extraction.CUSTOM_RTL_MIN
// @pypdf: pypdf._text_extraction.CUSTOM_RTL_MAX
// @pypdf: pypdf._text_extraction.CUSTOM_RTL_SPECIAL_CHARS
// @pypdf: pypdf._text_extraction.LAYOUT_NEW_BT_GROUP_SPACE_WIDTHS
// @pypdf: pypdf._text_extraction.UNICODE_LOWER_LIMIT
// @pypdf: pypdf._text_extraction.UNICODE_UPPER_LIMIT
// @pypdf: pypdf._text_extraction.RTL_CHARACTER_PATTERN
// @pypdf: pypdf._text_extraction._TEXT_OPERATORS
// @pypdf: pypdf._text_extraction._TEXT_OPERATOR_START
// @pypdf: pypdf._text_extraction.OrientationNotFoundError
// @pypdf: pypdf._text_extraction._layout_mode.__all__
// @pypdf: pypdf._text_extraction._layout_mode._fixed_width_page.WHITESPACE_LIMIT
// @pypdf: pypdf._text_extraction._layout_mode._fixed_width_page.NEWLINE_LIMIT
// @pypdf: pypdf._text_extraction._layout_mode._fixed_width_page.BTGroup
// @pypdf: pypdf._text_extraction._layout_mode._fixed_width_page.BTGroup.tx
// @pypdf: pypdf._text_extraction._layout_mode._fixed_width_page.BTGroup.ty
// @pypdf: pypdf._text_extraction._layout_mode._fixed_width_page.BTGroup.font_size
// @pypdf: pypdf._text_extraction._layout_mode._fixed_width_page.BTGroup.font_height
// @pypdf: pypdf._text_extraction._layout_mode._fixed_width_page.BTGroup.text
// @pypdf: pypdf._text_extraction._layout_mode._fixed_width_page.BTGroup.displaced_tx
// @pypdf: pypdf._text_extraction._layout_mode._fixed_width_page.BTGroup.flip_sort
// @pypdf: pypdf._text_extraction._layout_mode._text_state_manager.TextStateManager
// @pypdf: pypdf._text_extraction._layout_mode._text_state_manager.TextStateManagerKeyType
// @pypdf: pypdf._text_extraction._layout_mode._text_state_manager.TextStateManagerChainMapType
// @pypdf: pypdf._text_extraction._layout_mode._text_state_manager.TextStateManagerDictType
// @pypdf: pypdf._text_extraction._layout_mode._text_state_params.TextStateParams
// @pypdf: pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.value
// @pypdf: pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.font
// @pypdf: pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.font_size
// @pypdf: pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.Tc
// @pypdf: pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.Tw
// @pypdf: pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.Tz
// @pypdf: pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.TL
// @pypdf: pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.Ts
// @pypdf: pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.transform
// @pypdf: pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.tx
// @pypdf: pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.ty
// @pypdf: pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.displaced_tx
// @pypdf: pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.space_tx
// @pypdf: pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.font_height
// @pypdf: pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.flip_vertical
// @pypdf: pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.rotated
// @pypdf: pypdf._text_extraction._layout_mode._text_state_params.TextStateParams.text
// @pypdf: pypdf._text_extraction._layout_mode._text_state_params.TextStateParams._raw_chars
// @pypdf: pypdf._text_extraction._text_extractor.TextExtraction
