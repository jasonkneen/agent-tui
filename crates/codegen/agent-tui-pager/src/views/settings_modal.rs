//! Settings modal — opens via F2, `/settings`, command palette, and
//! shortcuts-help.
//!
//! ## State machine
//!
//! `SettingsModalState` carries a `UiConfig` snapshot plus a mode machine:
//!
//! - `Browse` — j/k navigates rows; Space toggles Bool; Enter opens
//!   a chooser/editor for Enum/String/Int.
//! - `FilterFocused` — `/` enters filter mode; `invalidate_filter`
//!   recomputes `filtered_cache` on every mutation.
//! - `PickingEnum { ... }` — enum chooser sub-pane.
//! - `EditingValue { ... }` — inline string/int editor.
//!
//! ## Keyboard ↔ mouse parity
//!
//! Every keyboard interaction has a mouse equivalent via `handle_mouse`.
//!
//! ## Close-key interception
//!
//! F2/Ctrl+,/Cmd+, are intercepted before mode-specific routing.
//! Esc-in-Browse is handled by the `ModalWindow` chrome (so
//! `is_close_key` does NOT match Esc); Esc-in-FilterFocused exits
//! filter mode without closing.

use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::app::actions::Action;
use crate::render::line_utils::truncate_str;
use crate::settings::{
    EnumChoice, OwnedEnumChoice, PagerLocalSnapshot, SettingCategory, SettingKey, SettingKind,
    SettingMeta, SettingValue, SettingsRegistry, StringValidator, current_value_for,
    dynamic_enum_choices,
};
use crate::theme::Theme;
use crate::views::modal_window::{
    self, ModalContentArea, ModalSizing, ModalWindowConfig, ModalWindowState, Shortcut,
};

use agent_tui_shell::agent::config::UiConfig;

// ---------------------------------------------------------------------------
// Public constants
// ---------------------------------------------------------------------------

/// Public display title of the modal — also used by
/// `views/modal.rs::ActiveModal::message` so renames stay in one place.
pub const MODAL_TITLE: &str = "Settings";

/// Width of the `"─ "` leading decoration before the title in the
/// modal's top border. Used to compute the breadcrumb hit-rect x offset.
const TITLE_LEADING_DECORATION_W: u16 = 2; // `─ `: 1 cell box-drawing + 1 cell space.

// Descriptions are now expand-on-demand via Right/Left arrows;
// see `render_expanded_description`.

/// Below this width the row list is skipped (chrome renders empty).
const CONTENT_MIN_WIDTH: u16 = 10;

/// Default max width for the modal. Keeps the row list compact on wide terminals.
const STANDARD_MAX_WIDTH: u16 = 110;

/// Per-side margin when editing `max_thoughts_width` (modal widens
/// to `terminal_width - 2*margin` so the wrap preview is useful).
const MAX_THOUGHTS_WIDTH_WIDENED_MARGIN: u16 = 8;

/// Outcome of a key or mouse event. Separate from `InputOutcome`
/// because the modal doesn't own `agent.active_modal` — close is
/// the caller's responsibility.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum SettingsKeyOutcome {
    /// Close the modal.
    Close,
    /// Forward to dispatch.
    Action(Action),
    /// Forward two actions in order (first must resolve before second).
    /// Used by `d`-reset-in-picker to revert preview before opening
    /// the reset-confirm overlay.
    ActionPair(Action, Action),
    /// Internal state mutation, no action.
    Changed,
    /// No-op.
    Unchanged,
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// One row in the visible flat list — either a category header (non-
/// selectable) or a setting row (selectable, dispatchable).
#[derive(Debug, Clone)]
pub enum RowEntry {
    Header { category: SettingCategory },
    Setting { key: SettingKey, meta_index: usize },
}

/// Mode state for the modal.
#[derive(Debug, Clone)]
pub enum SettingsModalMode {
    Browse,
    /// `/` was pressed; chars filter the visible rows.
    FilterFocused,
    /// Enum chooser sub-pane. `supports_preview` is cached at open
    /// time to avoid per-keystroke registry lookups.
    PickingEnum {
        key: SettingKey,
        choices_idx: usize,
        original_value: SettingValue,
        supports_preview: bool,
    },
    /// Group sub-sheet: a list of the group's child Bool toggles. `child_idx`
    /// is the focused child within the group. Space/Enter toggles in place
    /// (the sheet stays open); Esc returns to Browse. Mirrors `PickingEnum`'s
    /// open/render/commit flow but for independent toggles.
    PickingGroup {
        key: SettingKey,
        child_idx: usize,
    },
    /// Inline string/int editor. `cursor_byte` is always on a char
    /// boundary. `validation_error` shows live feedback; commit
    /// re-validates before dispatching. No `original_value` — these
    /// settings have no live preview, so Esc is a pure cancel.
    EditingValue {
        key: SettingKey,
        buffer: String,
        cursor_byte: usize,
        validation_error: Option<String>,
    },
}

/// Settings modal state. Boxed inside `ActiveModal::Settings` to
/// avoid clippy `large_enum_variant`.
pub struct SettingsModalState {
    pub window: ModalWindowState,
    pub registry: Arc<SettingsRegistry>,
    /// `UiConfig` snapshot, refreshed by the dispatcher on mutations.
    pub ui_snapshot: UiConfig,
    pub pager_snapshot: PagerLocalSnapshot,
    /// Computed row layout (headers + settings, in render order).
    pub rows: Vec<RowEntry>,
    /// Index into `rows` of the focused row.
    pub selected: usize,
    /// Vertical scroll offset (line-granular).
    pub scroll_offset: usize,
    pub mode: SettingsModalMode,
    /// Filter query. Persists across FilterFocused→Browse on Enter; cleared by Esc.
    pub query: String,
    /// Byte offset of the editing cursor within `query`.
    pub query_cursor: usize,
    /// Row indices matching `query`, recomputed per mutation (not per frame).
    filtered_cache: Vec<usize>,

    // -- Mouse hit-test rects (populated by render) --
    pub list_area: Rect,
    /// Click-hit rect per row, parallel to `rows`.
    pub row_rects: Vec<Rect>,
    /// Click-hit rect for the value column on each row. Bool rows
    /// toggle on click; Enum/String/Int rows open the sub-pane.
    pub value_hit_rects: Vec<Rect>,
    /// `(decrement_rect, increment_rect)` for the Int stepper's
    /// `‹`/`›` glyphs. Zero-sized when not in Int editing mode.
    pub editor_adornment_rects: (Rect, Rect),
    /// Click-hit rect per choice in `PickingEnum`. Each rect spans the
    /// full height of a choice (including wrapped description lines).
    pub picker_choice_rects: Vec<Rect>,
    /// Hit-rect for the breadcrumb title in sub-pane modes
    /// (`PickingEnum`/`EditingValue`). Clicking anywhere on
    /// `Settings › <label>` cancels back to Browse. `None` in
    /// Browse/FilterFocused. Cleared on mode transitions.
    pub settings_breadcrumb_rect: Option<Rect>,
    /// Hover flag for the breadcrumb — adds underline affordance.
    pub breadcrumb_hovered: bool,
    /// Keys whose description is expanded (Right/l to expand, Left/h
    /// to collapse). Multiple rows can be expanded simultaneously.
    pub expanded_keys: std::collections::HashSet<&'static str>,
    /// Row under the mouse cursor for hover highlighting. Indexes
    /// `rows` in Browse, `picker_choice_rects` in PickingEnum,
    /// always `None` in EditingValue.
    pub hover_row: Option<usize>,
}

impl SettingsModalState {
    /// Construct a new modal state from a registry + snapshots.
    pub fn new(
        registry: Arc<SettingsRegistry>,
        ui_snapshot: UiConfig,
        pager_snapshot: PagerLocalSnapshot,
    ) -> Self {
        let rows = build_rows(&registry);
        // Start on the first selectable (non-header) row.
        let selected = rows
            .iter()
            .position(|r| matches!(r, RowEntry::Setting { .. }))
            .unwrap_or(0);
        let filtered_cache = compute_filtered(&rows, &registry, "");
        Self {
            window: ModalWindowState::new(),
            registry,
            ui_snapshot,
            pager_snapshot,
            rows,
            selected,
            scroll_offset: 0,
            mode: SettingsModalMode::Browse,
            query: String::new(),
            query_cursor: 0,
            filtered_cache,
            list_area: Rect::default(),
            row_rects: Vec::new(),
            value_hit_rects: Vec::new(),
            editor_adornment_rects: (Rect::default(), Rect::default()),
            picker_choice_rects: Vec::new(),
            settings_breadcrumb_rect: None,
            breadcrumb_hovered: false,
            expanded_keys: std::collections::HashSet::new(),
            hover_row: None,
        }
    }

    /// The currently-focused setting row, if any.
    pub fn focused_setting(&self) -> Option<(SettingKey, &SettingMeta)> {
        match self.rows.get(self.selected)? {
            RowEntry::Setting { key, meta_index } => {
                let meta = self.registry.all().get(*meta_index)?;
                Some((*key, meta))
            }
            RowEntry::Header { .. } => None,
        }
    }

    /// Filtered row indices in render order.
    pub fn filtered_indices(&self) -> &[usize] {
        &self.filtered_cache
    }

    /// Recompute `filtered_cache` from the current `query`.
    fn invalidate_filter(&mut self) {
        self.filtered_cache = compute_filtered(&self.rows, &self.registry, &self.query);
    }

    /// Snap `selected` to the first visible setting if filtered out.
    fn clamp_selected_to_visible(&mut self) {
        if self.filtered_cache.is_empty() {
            return;
        }
        if self.filtered_cache.contains(&self.selected) {
            return;
        }
        // Snap to first selectable row in the visible filter.
        for &row_idx in &self.filtered_cache {
            if matches!(self.rows[row_idx], RowEntry::Setting { .. }) {
                self.selected = row_idx;
                return;
            }
        }
    }

    /// Read the current value for a setting key.
    pub fn value_for(&self, key: SettingKey) -> Option<SettingValue> {
        current_value_for(key, &self.ui_snapshot, &self.pager_snapshot)
    }

    /// Move `selected` forward, skipping headers and filtered-out rows.
    fn advance_next(&mut self) -> bool {
        let cur_pos = self.filtered_cache.iter().position(|&i| i == self.selected);
        let mut next = match cur_pos {
            Some(p) => p + 1,
            // Defensive: resume from top if `selected` is hidden.
            None => 0,
        };
        while next < self.filtered_cache.len() {
            let row_idx = self.filtered_cache[next];
            if matches!(self.rows[row_idx], RowEntry::Setting { .. }) {
                self.selected = row_idx;
                return true;
            }
            next += 1;
        }
        false
    }

    /// Move `selected` backward, skipping headers and filtered-out rows.
    fn advance_prev(&mut self) -> bool {
        if self.filtered_cache.is_empty() {
            return false;
        }
        let cur_pos = self.filtered_cache.iter().position(|&i| i == self.selected);
        let mut prev = match cur_pos {
            Some(p) if p > 0 => p - 1,
            Some(_) => return false,
            // Defensive: resume from bottom if `selected` is hidden.
            None => self.filtered_cache.len() - 1,
        };
        loop {
            let row_idx = self.filtered_cache[prev];
            if matches!(self.rows[row_idx], RowEntry::Setting { .. }) {
                self.selected = row_idx;
                return true;
            }
            if prev == 0 {
                break;
            }
            prev -= 1;
        }
        false
    }

    /// Set selection to `idx` if it's a selectable row.
    pub fn select_at(&mut self, idx: usize) -> bool {
        if idx >= self.rows.len() {
            return false;
        }
        if !matches!(self.rows[idx], RowEntry::Setting { .. }) {
            return false;
        }
        if self.selected == idx {
            return false;
        }
        self.selected = idx;
        true
    }

    /// Reset hit-test geometry so mouse handlers degrade gracefully
    /// when render is aborted. Does NOT clear `hover_row` — that's
    /// cleared on mode transitions instead to avoid per-frame flicker.
    pub(crate) fn reset_hit_rects(&mut self) {
        self.list_area = Rect::default();
        self.row_rects.clear();
        self.value_hit_rects.clear();
        self.editor_adornment_rects = (Rect::default(), Rect::default());
        self.picker_choice_rects.clear();
        self.settings_breadcrumb_rect = None;
        self.breadcrumb_hovered = false;
    }

    /// Transition to Browse, clearing sub-pane hover/breadcrumb state
    /// to prevent stale hit-rects across mode changes.
    pub(crate) fn transition_to_browse(&mut self) {
        self.mode = SettingsModalMode::Browse;
        self.hover_row = None;
        self.settings_breadcrumb_rect = None;
        self.breadcrumb_hovered = false;
    }

    /// Transition to `PickingEnum` if the focused row is Enum/DynamicEnum.
    /// Returns `false` if the focused row is another kind.
    pub fn try_enter_picking_enum(&mut self) -> bool {
        let (key, first_canonical, current_value, supports_preview, resolved_choices) = {
            let Some((key, meta)) = self.focused_setting() else {
                return false;
            };
            // Handles both static `Enum` and `DynamicEnum` catalogs.
            let (supports_preview, resolved): (bool, Vec<OwnedEnumChoice>) = match &meta.kind {
                SettingKind::Enum {
                    choices,
                    supports_preview,
                    ..
                } => (
                    *supports_preview,
                    effective_enum_choices(key, choices, &self.pager_snapshot)
                        .into_iter()
                        .map(|c| OwnedEnumChoice {
                            canonical: c.canonical.to_string(),
                            display: c.display.to_string(),
                            description: c.description.to_string(),
                        })
                        .collect(),
                ),
                SettingKind::DynamicEnum {
                    source,
                    supports_preview,
                    ..
                } => (
                    *supports_preview,
                    dynamic_enum_choices(*source, &self.pager_snapshot),
                ),
                _ => return false,
            };
            // Soft-fail if a static catalog exceeds the product cap. DynamicEnum
            // (e.g. models) is exempt — those lists are runtime-sized and always
            // scroll. The chooser itself scrolls static lists too; this assert is
            // a design guard, not a render requirement.
            debug_assert!(
                resolved.len() <= MAX_PICKER_CHOICES
                    || matches!(meta.kind, SettingKind::DynamicEnum { .. }),
                "Static Enum setting `{}` has {} choices, exceeds MAX_PICKER_CHOICES ({}). \
                 Raise the cap deliberately if a larger curated catalog is required.",
                key,
                resolved.len(),
                MAX_PICKER_CHOICES,
            );
            let first = resolved
                .first()
                .map(|c| c.canonical.clone())
                .unwrap_or_default();
            let cur = self.value_for(key);
            (key, first, cur, supports_preview, resolved)
        };

        // Resolve choices_idx from current value. For DynamicEnum,
        // if the current value no longer exists in the catalog,
        // fall back to index 1 (first real entry past sentinel)
        // to avoid accidentally wiping the user's preference.
        let is_dynamic_enum = matches!(
            self.registry.find(key).map(|m| &m.kind),
            Some(SettingKind::DynamicEnum { .. })
        );
        let unknown_dynamic_fallback_idx = if is_dynamic_enum && resolved_choices.len() > 1 {
            1
        } else {
            0
        };
        let choices_idx = match &current_value {
            Some(SettingValue::Enum(cur)) => resolved_choices
                .iter()
                .position(|c| c.canonical == *cur)
                .unwrap_or(0),
            Some(SettingValue::String(cur)) if !cur.is_empty() => resolved_choices
                .iter()
                .position(|c| c.canonical == *cur)
                .unwrap_or(unknown_dynamic_fallback_idx),
            Some(SettingValue::String(_)) => 0,
            _ => 0,
        };
        if is_dynamic_enum
            && choices_idx == unknown_dynamic_fallback_idx
            && unknown_dynamic_fallback_idx != 0
        {
            // Telemetry: log when a DynamicEnum value is stale.
            tracing::warn!(
                target: "settings",
                key = key,
                ?current_value,
                "DynamicEnum picker entered with a current value that no longer resolves \
                 in the live catalog — focusing first real choice instead of the \
                 (no override) sentinel to defend against accidental destructive Enter",
            );
        }
        let original_value = current_value.unwrap_or_else(|| {
            // Fallback to first choice, using the right value carrier.
            match self.registry.find(key).map(|m| &m.kind) {
                Some(SettingKind::DynamicEnum { .. }) => SettingValue::String(first_canonical),
                Some(SettingKind::Enum { choices, .. }) => {
                    let first_static = choices.first().map(|c| c.canonical).unwrap_or("");
                    SettingValue::Enum(first_static)
                }
                _ => SettingValue::Enum(""),
            }
        });
        self.mode = SettingsModalMode::PickingEnum {
            key,
            choices_idx,
            supports_preview,
            original_value,
        };
        self.hover_row = None;
        true
    }

    /// Transition to `PickingGroup` if the focused row is a `Group`. Returns
    /// `false` for any other kind so the caller can fall through to the
    /// enum/editor entry points.
    pub fn try_enter_picking_group(&mut self) -> bool {
        let Some((key, meta)) = self.focused_setting() else {
            return false;
        };
        if !matches!(meta.kind, SettingKind::Group { .. }) {
            return false;
        }
        self.mode = SettingsModalMode::PickingGroup { key, child_idx: 0 };
        self.hover_row = None;
        true
    }

    /// Transition to `EditingValue` if the focused row is String or Int.
    pub fn try_enter_editing_value(&mut self) -> bool {
        let Some((key, meta)) = self.focused_setting() else {
            return false;
        };
        let buffer = match (&meta.kind, self.value_for(key)) {
            (SettingKind::String { .. }, Some(SettingValue::String(s))) => s,
            (SettingKind::Int { .. }, Some(SettingValue::Int(i))) => i.to_string(),
            // Fallback for registry skew — seed from default.
            (SettingKind::String { default, .. }, _) => default.to_string(),
            (SettingKind::Int { default, .. }, _) => default.to_string(),
            _ => return false,
        };
        let cursor_byte = buffer.len();

        // Validate the seed value upfront.
        let validation_error = match &meta.kind {
            SettingKind::String { validator, .. } => {
                validate_string(*validator, &buffer, &self.pager_snapshot.available_models)
            }
            SettingKind::Int { min, max, .. } => validate_int(&buffer, *min, *max),
            _ => None,
        };

        self.mode = SettingsModalMode::EditingValue {
            key,
            buffer,
            cursor_byte,
            validation_error,
        };
        self.hover_row = None;
        true
    }

    /// Build the Action that toggles the focused Bool row. Returns
    /// `None` with an error log on registry skew (caught by CI tests).
    pub fn toggle_focused_bool(&self) -> Option<Action> {
        let (key, meta) = self.focused_setting()?;
        if !matches!(meta.kind, SettingKind::Bool { .. }) {
            return None;
        }
        let cur = match self.value_for(key) {
            Some(SettingValue::Bool(b)) => b,
            Some(other) => {
                tracing::error!(
                    target: "settings",
                    ?key,
                    ?other,
                    "Bool-kind setting resolved to non-Bool value — registry skew",
                );
                return None;
            }
            None => {
                tracing::error!(
                    target: "settings",
                    ?key,
                    "Bool-kind setting has no current_value_for arm — registry skew",
                );
                return None;
            }
        };
        let action = action_for_bool(key, !cur);
        if action.is_none() {
            tracing::error!(
                target: "settings",
                ?key,
                "Bool-kind setting has no action_for_bool arm — registry skew",
            );
        }
        action
    }
}

/// Compute filtered row indices for a query. Headers are emitted only
/// when ≥1 setting in their section matches. Returns all indices when
/// `query` is empty.
fn compute_filtered(rows: &[RowEntry], registry: &SettingsRegistry, query: &str) -> Vec<usize> {
    if query.is_empty() {
        return (0..rows.len()).collect();
    }
    let matched_keys: Vec<SettingKey> = registry.search(query).iter().map(|m| m.key).collect();
    let mut result = Vec::new();
    let mut pending_header: Option<usize> = None;
    for (i, row) in rows.iter().enumerate() {
        match row {
            RowEntry::Header { .. } => {
                // Emit header only when section has a match.
                pending_header = Some(i);
            }
            RowEntry::Setting { key, .. } => {
                if matched_keys.contains(key) {
                    if let Some(h) = pending_header.take() {
                        result.push(h);
                    }
                    result.push(i);
                }
            }
        }
    }
    result
}

/// Per-mode / per-terminal row visibility. `voice_capture_mode` is hidden
/// without key-release reporting (capture is always toggle there, so there's
/// no choice). Pure (`kitty_releases` / `minimal` passed in) so it's testable
/// without process globals.
fn setting_row_visible(meta: &SettingMeta, kitty_releases: bool, minimal: bool) -> bool {
    if meta.key == "voice_capture_mode" && !kitty_releases {
        return false;
    }
    if minimal && meta.hidden_in_minimal {
        return false;
    }
    true
}

fn build_rows(registry: &SettingsRegistry) -> Vec<RowEntry> {
    let kitty_releases = crate::app::kitty_flags_pushed();
    let minimal = crate::app::minimal_mode_active();
    // Keys that belong to a group sub-sheet are rendered only inside that
    // sheet, never as their own top-level rows.
    let group_children: std::collections::HashSet<SettingKey> = registry
        .all()
        .iter()
        .filter_map(|m| match &m.kind {
            SettingKind::Group { children } => Some(*children),
            _ => None,
        })
        .flatten()
        .copied()
        .collect();
    let mut rows = Vec::new();
    for cat in SettingCategory::ALL {
        let mut emitted_header = false;
        for (meta_index, meta) in registry.all().iter().enumerate() {
            if meta.category != *cat {
                continue;
            }
            if !setting_row_visible(meta, kitty_releases, minimal) {
                continue;
            }
            if group_children.contains(meta.key) {
                continue;
            }
            if !emitted_header {
                rows.push(RowEntry::Header { category: *cat });
                emitted_header = true;
            }
            rows.push(RowEntry::Setting {
                key: meta.key,
                meta_index,
            });
        }
    }
    rows
}

/// Construct the typed `Action::Set*` for a Bool setting.
fn action_for_bool(key: SettingKey, new: bool) -> Option<Action> {
    match key {
        "compact_mode" => Some(Action::SetCompactMode(new)),
        "show_timestamps" => Some(Action::SetTimestamps(new)),
        "simple_mode" => Some(Action::SetSimpleMode(new)),
        "contextual_hints.undo" => Some(Action::SetContextualHintUndo(new)),
        "contextual_hints.plan_mode" => Some(Action::SetContextualHintPlanMode(new)),
        "contextual_hints.image_input" => Some(Action::SetContextualHintImageInput(new)),
        "contextual_hints.send_now" => Some(Action::SetContextualHintSendNow(new)),
        "contextual_hints.small_screen" => Some(Action::SetContextualHintSmallScreen(new)),
        "contextual_hints.word_select" => Some(Action::SetContextualHintWordSelect(new)),
        "multiline_mode" => Some(Action::SetMultilineMode(new)),
        "vim_mode" => Some(Action::SetVimMode(new)),
        "remember_tool_approvals" => Some(Action::SetRememberToolApprovals(new)),
        "toolset.ask_user_question.timeout_enabled" => {
            Some(Action::SetAskUserQuestionTimeoutEnabled(new))
        }
        "show_thinking_blocks" => Some(Action::SetShowThinkingBlocks(new)),
        "group_tool_verbs" => Some(Action::SetGroupToolVerbs(new)),
        "collapsed_edit_blocks" => Some(Action::SetCollapsedEditBlocks(new)),
        "prompt_suggestions" => Some(Action::SetPromptSuggestions(new)),
        "respect_manual_folds" => Some(Action::SetRespectManualFolds(new)),
        "invert_scroll" => Some(Action::SetInvertScroll(new)),
        "show_tips" => Some(Action::SetShowTips(new)),
        "auto_update" => Some(Action::SetAutoUpdate(new)),
        "display_refresh_auto_cadence" => Some(Action::SetDisplayRefreshAutoCadence(new)),
        _ => None,
    }
}

/// Construct `Action::Preview*` for an Enum setting — used by the
/// picker's Up/Down (live preview) and Esc (revert). Preview actions
/// never persist; they only mutate the live visual.
fn action_for_enum(key: SettingKey, choice: &'static str) -> Option<Action> {
    match key {
        "theme" => Some(Action::PreviewTheme(choice.to_string())),
        "auto_dark_theme" => Some(Action::PreviewAutoDarkTheme(choice.to_string())),
        "auto_light_theme" => Some(Action::PreviewAutoLightTheme(choice.to_string())),
        // No preview for settings with irreversible side effects.
        "permission_mode" => None,
        "coding_data_sharing" => None,
        "plan_mode" => None,
        "render_mermaid" => None,
        "keep_text_selection" => None,
        "scroll_mode" => None,
        _ => None,
    }
}

/// Construct `Action::Set*` commit variant for an Enum setting.
/// Commit actions persist to disk and fire a toast.
fn action_for_enum_commit(key: SettingKey, choice: &'static str) -> Option<Action> {
    match key {
        "theme" => Some(Action::SetTheme(choice.to_string())),
        "auto_dark_theme" => Some(Action::SetAutoDarkTheme(choice.to_string())),
        "auto_light_theme" => Some(Action::SetAutoLightTheme(choice.to_string())),
        // Canonical strings from settings/defs.rs are the source of truth.
        "permission_mode" => match choice {
            "always-approve" => Some(Action::SetPermissionMode(
                crate::app::actions::PermissionModeKind::AlwaysApprove,
            )),
            // Auto's feature gate is enforced in `set_permission_mode`
            // (via `app.auto_mode_gate`, the same source the Shift+Tab cycle
            // uses), so the modal and the cycle never disagree. Committing Auto
            // when the gate is off degrades to Ask there.
            "auto" => Some(Action::SetPermissionMode(
                crate::app::actions::PermissionModeKind::Auto,
            )),
            "ask" => Some(Action::SetPermissionMode(
                crate::app::actions::PermissionModeKind::Ask,
            )),
            "default" => Some(Action::SetPermissionMode(
                crate::app::actions::PermissionModeKind::Default,
            )),
            _ => None,
        },
        "coding_data_sharing" => match choice {
            "opt-in" => Some(Action::SetCodingDataSharing { opted_in: true }),
            "opt-out" => Some(Action::SetCodingDataSharing { opted_in: false }),
            _ => None,
        },
        "plan_mode" => match choice {
            "on" => Some(Action::SetPlanMode(crate::app::actions::PlanModeKind::On)),
            "off" => Some(Action::SetPlanMode(crate::app::actions::PlanModeKind::Off)),
            _ => None,
        },
        "hunk_tracker_mode" => Some(Action::SetHunkTrackerMode(choice.to_string())),
        "voice_capture_mode" => Some(Action::SetVoiceCaptureMode(choice.to_string())),
        "voice_stt_language" => Some(Action::SetVoiceSttLanguage(choice.to_string())),
        "render_mermaid" => {
            crate::appearance::RenderMermaid::from_canonical(choice).map(Action::SetRenderMermaid)
        }
        "keep_text_selection" => crate::appearance::TextSelection::from_canonical(choice)
            .map(Action::SetKeepTextSelection),
        // Junk canonicals fold to None — Enter no-ops instead of mis-mapping.
        "scroll_mode" => {
            crate::appearance::ScrollMode::from_canonical(choice).map(Action::SetScrollMode)
        }
        "default_selected_permission" => {
            Some(Action::SetDefaultSelectedPermission(choice.to_string()))
        }
        _ => None,
    }
}

/// Construct `Action::Set*` commit variant for a String setting.
/// Resolves model names via the snapshot before producing the action.
/// Empty buffer maps to `Action::Clear*` for model settings.
fn action_for_string(
    key: SettingKey,
    value: String,
    snapshot: &PagerLocalSnapshot,
) -> Option<Action> {
    match key {
        "default_model" => {
            if value.is_empty() {
                Some(Action::ClearDefaultModel)
            } else {
                snapshot
                    .resolve_model_name(&value)
                    .map(Action::SetDefaultModel)
            }
        }
        "fork_secondary_model" => {
            if value.is_empty() {
                Some(Action::ClearForkSecondaryModel)
            } else {
                snapshot
                    .resolve_model_name(&value)
                    .map(Action::SetForkSecondaryModel)
            }
        }

        _ => {
            let _ = value;
            let _ = snapshot;
            None
        }
    }
}

/// Construct `Action::Set*` commit variant for an Int setting.
fn action_for_int(key: SettingKey, value: i64) -> Option<Action> {
    match key {
        "max_thoughts_width" => Some(Action::SetMaxThoughtsWidth(value)),
        "scroll_speed" => Some(Action::SetScrollSpeed(value)),
        "scroll_lines" => Some(Action::SetScrollLines(value)),
        _ => None,
    }
}

/// Validate a String buffer against the registered `StringValidator`.
/// Returns `Some(error_message)` on failure, `None` on success.
fn validate_string(
    validator: StringValidator,
    buffer: &str,
    available_models: &[(String, agent_client_protocol::ModelId)],
) -> Option<String> {
    match validator {
        StringValidator::Any => None,
        StringValidator::NonEmptyToken => {
            if buffer.is_empty() {
                Some("Value cannot be empty".to_string())
            } else if buffer.chars().any(|c| c.is_whitespace()) {
                Some("Value cannot contain whitespace".to_string())
            } else {
                None
            }
        }
        StringValidator::KnownModel => {
            // Empty = "clear default" sentinel.
            if buffer.is_empty() {
                return None;
            }
            // Reject if the model catalog hasn't loaded yet.
            if available_models.is_empty() {
                return Some("Model catalog still loading — try again".to_string());
            }
            let matched = available_models
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case(buffer));
            if matched {
                None
            } else {
                Some(format!("Unknown model: \"{buffer}\""))
            }
        }
    }
}

/// Validate an Int buffer against `(min, max)` bounds.
fn validate_int(buffer: &str, min: i64, max: i64) -> Option<String> {
    if buffer.is_empty() {
        return Some("Value cannot be empty".to_string());
    }
    match buffer.parse::<i64>() {
        Ok(v) if v >= min && v <= max => None,
        Ok(v) => Some(format!("Value out of range ({min}\u{2013}{max}): {v}")),
        Err(_) => Some(format!("Not a valid integer: \"{buffer}\"")),
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Overlay for the reset-confirm dialog. Overrides chrome breadcrumb,
/// footer, and search bar with the confirmation prompt.
pub struct ResetConfirmOverlay<'a> {
    pub prompt: &'a str,

    pub breadcrumb_suffix: &'a str,
}

/// Render the settings modal. Returns `true` if the reset-confirm
/// overlay was rendered (caller suppresses row-list mouse events).
pub fn render_settings_modal(
    buf: &mut Buffer,
    full_area: Rect,
    state: &mut SettingsModalState,
    compact: bool,
    overlay: Option<&ResetConfirmOverlay<'_>>,
) -> bool {
    let theme = Theme::current();
    let confirm_shortcuts = build_reset_confirm_shortcuts();
    let normal_shortcuts = build_shortcuts(state);
    let shortcuts: &[Shortcut<'_>] = if overlay.is_some() {
        &confirm_shortcuts
    } else {
        &normal_shortcuts
    };

    // Breadcrumb title for sub-modes: "Settings › <label>".
    let breadcrumb_owned: String;
    let title: &str = if let Some(o) = overlay {
        breadcrumb_owned = format!(
            "{MODAL_TITLE} {} {}",
            crate::glyphs::chevron(),
            o.breadcrumb_suffix
        );
        &breadcrumb_owned
    } else {
        match &state.mode {
            SettingsModalMode::PickingEnum { key, .. } => {
                if let Some(meta) = state.registry.find(key) {
                    breadcrumb_owned =
                        format!("{MODAL_TITLE} {} {}", crate::glyphs::chevron(), meta.label);
                    &breadcrumb_owned
                } else {
                    MODAL_TITLE
                }
            }

            SettingsModalMode::EditingValue { key, .. } => {
                if let Some(meta) = state.registry.find(key) {
                    breadcrumb_owned =
                        format!("{MODAL_TITLE} {} {}", crate::glyphs::chevron(), meta.label);
                    &breadcrumb_owned
                } else {
                    MODAL_TITLE
                }
            }
            SettingsModalMode::PickingGroup { key, .. } => {
                if let Some(meta) = state.registry.find(key) {
                    breadcrumb_owned =
                        format!("{MODAL_TITLE} {} {}", crate::glyphs::chevron(), meta.label);
                    &breadcrumb_owned
                } else {
                    MODAL_TITLE
                }
            }
            _ => MODAL_TITLE,
        }
    };

    // Footer sizing: predict shortcut wrap rows, add a gap row above
    // when the docs footer is present. EditingValue suppresses the
    // docs footer. Widen the modal when editing `max_thoughts_width`
    // so the wrap preview is useful at widths above STANDARD_MAX_WIDTH.
    let widen_for_max_thoughts_width = matches!(
        &state.mode,
        SettingsModalMode::EditingValue { key, .. }
            if *key == crate::settings::defs::MAX_THOUGHTS_WIDTH_KEY
    );
    let widened_candidate = full_area
        .width
        .saturating_sub(MAX_THOUGHTS_WIDTH_WIDENED_MARGIN);
    let (max_width, width_pct) =
        if widen_for_max_thoughts_width && widened_candidate > STANDARD_MAX_WIDTH {
            (widened_candidate, 1.0)
        } else {
            (STANDARD_MAX_WIDTH, 0.70)
        };
    let sizing = ModalSizing {
        width_pct,
        max_width,
        min_width: 44,
        v_margin: 3,
        h_pad: 2,
        v_pad: 1,
        footer_lines: 2,
    }
    .with_compact(compact);
    let has_tip_footer = !matches!(state.mode, SettingsModalMode::EditingValue { .. });
    let footer_lines = if has_tip_footer {
        modal_window::footer_lines_with_tip_gap(full_area, &sizing, shortcuts)
    } else {
        sizing.footer_lines
    };
    let sizing = ModalSizing {
        footer_lines,
        ..sizing
    };

    let modal_config = ModalWindowConfig {
        title,
        tabs: None,
        shortcuts,
        sizing,
        fold_info: None,
    };

    let Some(ModalContentArea {
        content: content_area,
        ..
    }) =
        modal_window::render_modal_window(buf, full_area, &mut state.window, &modal_config, &theme)
    else {
        // Chrome refused — reset hit-rects for graceful degradation.
        state.reset_hit_rects();
        return overlay.is_some();
    };

    if content_area.height < 2 || content_area.width < CONTENT_MIN_WIDTH {
        state.reset_hit_rects();
        return overlay.is_some();
    }

    if let Some(o) = overlay {
        // Confirmation overlay replaces the search bar; row list
        // renders dimmed underneath. Hit-rects reset so clicks
        // only route to the y/n footer buttons.
        render_reset_confirm_overlay(buf, content_area, state, &theme, o);
        return true;
    }

    let (inner_area, docs_footer_area) = match state.mode {
        SettingsModalMode::EditingValue { .. } => (content_area, None),
        _ => modal_window::split_content_for_tip_footer(content_area),
    };

    // Per-mode render dispatch (exhaustive to catch new variants).
    let mode_is_sub_pane = matches!(
        state.mode,
        SettingsModalMode::PickingEnum { .. }
            | SettingsModalMode::PickingGroup { .. }
            | SettingsModalMode::EditingValue { .. }
    );
    match state.mode {
        SettingsModalMode::PickingEnum { .. } => {
            state.reset_hit_rects();
            render_picking_enum(buf, inner_area, state, &theme);
            state.picker_choice_rects = take_picker_choice_rects();
        }
        SettingsModalMode::PickingGroup { .. } => {
            state.reset_hit_rects();
            let rects = render_picking_group(buf, inner_area, state, &theme);
            state.picker_choice_rects = rects;
        }
        SettingsModalMode::EditingValue { .. } => {
            state.reset_hit_rects();
            render_editing_value(buf, inner_area, state, &theme);
        }
        SettingsModalMode::Browse | SettingsModalMode::FilterFocused => {
            // Clear sub-pane hit-rects from prior frames.
            state.picker_choice_rects.clear();
            state.editor_adornment_rects = (Rect::default(), Rect::default());
            state.settings_breadcrumb_rect = None;
            state.list_area = inner_area;
            render_row_list_with_search_bar(buf, inner_area, state, &theme);
        }
    }

    // Breadcrumb hit-rect for sub-pane back-navigation on click.
    // Repaint with UNDERLINED (+ accent on hover) for affordance.
    state.settings_breadcrumb_rect = if mode_is_sub_pane {
        state.window.popup_area.map(|popup| {
            let title_w = title.width() as u16;
            // Clamp to not extend past the close button.
            let max_w = popup.width.saturating_sub(2 + 2); // borders + " ─" trailing decoration
            Rect {
                x: popup.x + 1 + TITLE_LEADING_DECORATION_W,
                y: popup.y,
                width: title_w.min(max_w),
                height: 1,
            }
        })
    } else {
        None
    };
    if let Some(rect) = state.settings_breadcrumb_rect {
        let fg = if state.breadcrumb_hovered {
            theme.accent_user
        } else {
            theme.text_primary
        };
        let style_mods = Modifier::BOLD | Modifier::UNDERLINED;
        for offset in 0..rect.width {
            let x = rect.x + offset;
            if let Some(cell) = buf.cell_mut((x, rect.y)) {
                let mut s = cell.style();
                s.fg = Some(fg);
                s = s.add_modifier(style_mods);
                cell.set_style(s);
            }
        }
    }

    if let Some(footer_area) = docs_footer_area {
        render_docs_footer(buf, footer_area, &theme);
    }
    false
}

/// Render the reset-confirm overlay: prompt replaces the search bar,
/// row list renders below with the target row at full intensity and
/// all other rows dimmed.
fn render_reset_confirm_overlay(
    buf: &mut Buffer,
    content_area: Rect,
    state: &mut SettingsModalState,
    theme: &Theme,
    overlay: &ResetConfirmOverlay<'_>,
) {
    // Clear hit-rects so clicks only route to y/n buttons.
    state.reset_hit_rects();

    if content_area.height == 0 || content_area.width == 0 {
        return;
    }

    // Row 0: prompt (full width, bold + accent).
    let prompt_area = Rect {
        x: content_area.x,
        y: content_area.y,
        width: content_area.width,
        height: 1,
    };
    let prompt_bg_style = Style::default().bg(theme.bg_visual);
    buf.set_style(prompt_area, prompt_bg_style);
    let prompt_style = Style::default()
        .fg(theme.accent_user)
        .bg(theme.bg_visual)
        .add_modifier(Modifier::BOLD);
    let prompt_text: std::borrow::Cow<'_, str> =
        if overlay.prompt.width() <= content_area.width as usize {
            std::borrow::Cow::Borrowed(overlay.prompt)
        } else {
            std::borrow::Cow::Owned(truncate_str(overlay.prompt, content_area.width as usize))
        };
    let prompt_w = (prompt_text.width() as u16).min(content_area.width);
    buf.set_span(
        content_area.x,
        content_area.y,
        &Span::styled(prompt_text.as_ref(), prompt_style),
        prompt_w,
    );

    // Row 1+: render rows, then dim all except the target.
    if content_area.height < 2 {
        return;
    }
    let list_area = Rect {
        x: content_area.x,
        y: content_area.y + 1,
        width: content_area.width,
        height: content_area.height - 1,
    };
    render_rows(buf, list_area, state, theme);

    let target_rect = state.row_rects.get(state.selected).copied();

    // Dim every line outside the target row's y-range (DIM + blend).
    let target_y_start = target_rect.map(|r| r.y);
    let target_y_end = target_rect.map(|r| r.y.saturating_add(r.height));
    let area_y_end = list_area.y.saturating_add(list_area.height);
    for y in list_area.y..area_y_end {
        if let (Some(ys), Some(ye)) = (target_y_start, target_y_end)
            && y >= ys
            && y < ye
        {
            continue; // inside the target row's y range — stays full intensity
        }
        let strip = Rect {
            x: list_area.x,
            y,
            width: list_area.width,
            height: 1,
        };
        buf.set_style(strip, Style::default().add_modifier(Modifier::DIM));
        crate::render::color::blend_area(buf, strip, Some((theme.bg_base, 0.55)), None);
    }
}

/// Footer shortcuts for the reset-confirm dialog (y/n are clickable).
fn build_reset_confirm_shortcuts() -> Vec<Shortcut<'static>> {
    use crate::views::modal::{RESET_CONFIRM_NO_ID, RESET_CONFIRM_YES_ID};
    vec![
        Shortcut {
            label: "y reset",
            clickable: true,
            id: RESET_CONFIRM_YES_ID,
        },
        Shortcut {
            label: "n cancel",
            clickable: true,
            id: RESET_CONFIRM_NO_ID,
        },
        Shortcut {
            label: "Esc cancel",
            clickable: false,
            id: 0,
        },
        Shortcut {
            label: "F2 cancel",
            clickable: false,
            id: 0,
        },
    ]
}

/// Render the row list with a search bar at the top (Browse/FilterFocused).
fn render_row_list_with_search_bar(
    buf: &mut Buffer,
    content_area: Rect,
    state: &mut SettingsModalState,
    theme: &Theme,
) {
    let filter_focused = matches!(state.mode, SettingsModalMode::FilterFocused);
    if content_area.height >= 3 {
        // row 0: search bar, row 1: divider, row 2+: list.
        let search_area = Rect {
            x: content_area.x,
            y: content_area.y,
            width: content_area.width,
            height: 1,
        };
        crate::views::picker::render_search_bar(
            buf,
            search_area.x,
            search_area.y,
            search_area.width,
            theme,
            &state.query,
            filter_focused,
            true,
            state.query_cursor,
            Some(theme.bg_base),
        );
        crate::views::picker::render_divider(
            buf,
            content_area.x,
            content_area.y + 1,
            content_area.width,
            theme,
            Some(theme.bg_base),
        );
        let list_area = Rect {
            x: content_area.x,
            y: content_area.y + 2,
            width: content_area.width,
            height: content_area.height - 2,
        };

        state.list_area = list_area;
        render_rows(buf, list_area, state, theme);
    } else if content_area.height >= 2 {
        // Tight: search bar only, no divider.
        let search_area = Rect {
            x: content_area.x,
            y: content_area.y,
            width: content_area.width,
            height: 1,
        };
        crate::views::picker::render_search_bar(
            buf,
            search_area.x,
            search_area.y,
            search_area.width,
            theme,
            &state.query,
            filter_focused,
            true,
            state.query_cursor,
            Some(theme.bg_base),
        );
        let list_area = Rect {
            x: content_area.x,
            y: content_area.y + 1,
            width: content_area.width,
            height: content_area.height - 1,
        };
        state.list_area = list_area;
        render_rows(buf, list_area, state, theme);
    } else {
        // Too narrow for a search bar; just render the rows.
        render_rows(buf, content_area, state, theme);
    }
}

fn render_docs_footer(buf: &mut Buffer, area: Rect, theme: &Theme) {
    const LONG: &str =
        "Tip · Ask Grok: \"change theme to grokday\" or \"what does compact mode do?\"";
    const SHORT: &str = "Tip · Ask Grok to change a setting";
    let text = modal_window::fit_tip_line(&[LONG, SHORT], area.width as usize);
    modal_window::render_centered_tip_footer(buf, area, theme, text.as_ref());
}

fn render_rows(buf: &mut Buffer, area: Rect, state: &mut SettingsModalState, theme: &Theme) {
    let visible_h = area.height as usize;
    if visible_h == 0 {
        state.row_rects.clear();
        state.row_rects.resize(state.rows.len(), Rect::default());
        state.value_hit_rects.clear();
        state
            .value_hit_rects
            .resize(state.rows.len(), Rect::default());
        return;
    }

    state.row_rects.clear();
    state.row_rects.resize(state.rows.len(), Rect::default());
    state.value_hit_rects.clear();
    state
        .value_hit_rects
        .resize(state.rows.len(), Rect::default());

    let total_visible = state.filtered_cache.len();

    // Empty filter — show "No matches for <query>".
    if total_visible == 0 {
        if !state.query.is_empty() {
            let prefix = "No matches for ";
            let suffix_quote_w = 2u16; // surrounding "" chars
            let available_for_query = (area.width as usize)
                .saturating_sub(prefix.width())
                .saturating_sub(suffix_quote_w as usize);
            let q_disp = if state.query.width() <= available_for_query {
                state.query.clone()
            } else {
                truncate_str(&state.query, available_for_query)
            };
            let msg = format!("{prefix}\"{q_disp}\"");
            let style = Style::default().fg(theme.gray_dim).bg(theme.bg_base);
            let msg_w = (msg.width() as u16).min(area.width);
            let cx = area.x + area.width.saturating_sub(msg_w) / 2;
            let cy = area.y + area.height / 2;
            buf.set_span(cx, cy, &Span::styled(&msg, style), msg_w);
        }
        return;
    }

    // Translate `state.selected` (rows-space) → filtered position.
    let selected_fpos = state
        .filtered_cache
        .iter()
        .position(|&i| i == state.selected);

    // Clamp scroll so selection stays in view, keeping the preceding
    // section header visible when scrolling up. Row heights are
    // variable (expanded descriptions, header gaps).
    let row_heights = compute_filtered_row_heights(state, area.width);
    if let Some(fpos) = selected_fpos {
        if fpos < state.scroll_offset {
            let new_offset = if fpos > 0 {
                let prev_idx = state.filtered_cache[fpos - 1];
                if matches!(state.rows[prev_idx], RowEntry::Header { .. }) {
                    fpos - 1
                } else {
                    fpos
                }
            } else {
                fpos
            };
            state.scroll_offset = new_offset;
        }

        let min_offset_for_visibility = compute_min_scroll_offset_for_visibility(
            &state.filtered_cache,
            &state.rows,
            &row_heights,
            fpos,
            visible_h,
        );
        if state.scroll_offset < min_offset_for_visibility {
            state.scroll_offset = min_offset_for_visibility;
        }
    }
    // Final clamp — don't let scroll_offset past the end.
    if total_visible > 0 {
        let max_offset = compute_min_scroll_offset_for_visibility(
            &state.filtered_cache,
            &state.rows,
            &row_heights,
            total_visible - 1,
            visible_h,
        );
        if state.scroll_offset > max_offset {
            state.scroll_offset = max_offset;
        }
    }

    let end = total_visible.min(state.scroll_offset + visible_h);

    let max_label_w = compute_settings_max_label_w(state.registry.all(), area.width);

    // Snapshot visible rows to avoid borrow conflicts in the render loop.
    let visible_filtered: Vec<usize> = state.filtered_cache[state.scroll_offset..end].to_vec();

    let hover_row_snapshot = state.hover_row;
    let mut values: Vec<Option<SettingValue>> = Vec::with_capacity(visible_filtered.len());
    for &row_idx in &visible_filtered {
        let v = match state.rows.get(row_idx) {
            Some(RowEntry::Setting { key, .. }) => state.value_for(key),
            _ => None,
        };
        values.push(v);
    }
    // Track y-cursor: rows consume variable height when expanded.
    let mut y_cursor = area.y;
    let area_end = area.y + area.height;
    let expanded_snapshot: std::collections::HashSet<&'static str> = state.expanded_keys.clone();
    // Insert a blank line above non-first section headers.
    let mut rendered_any = false;

    for (row_pos, &row_idx) in visible_filtered.iter().enumerate() {
        if y_cursor >= area_end {
            break;
        }
        let Some(row) = state.rows.get(row_idx) else {
            continue;
        };

        if matches!(row, RowEntry::Header { .. })
            && rendered_any
            && y_cursor.saturating_add(1) < area_end
        {
            y_cursor = y_cursor.saturating_add(1);
        }
        if y_cursor >= area_end {
            break;
        }
        let label_rect = Rect {
            x: area.x,
            y: y_cursor,
            width: area.width,
            height: 1,
        };

        state.row_rects[row_idx] = label_rect;

        rendered_any = true;

        match row {
            RowEntry::Header { category } => {
                let label = category.label();
                let header_style = Style::default()
                    .fg(theme.gray)
                    .bg(theme.bg_base)
                    .add_modifier(Modifier::BOLD);
                let sep_style = Style::default().fg(theme.gray_dim).bg(theme.bg_base);
                let title = format!(" {label} ");
                let title_w = title.width();
                let remaining = (area.width as usize).saturating_sub(title_w);
                let sep: String = std::iter::repeat_n('\u{2500}', remaining).collect();
                let line = Line::from(vec![
                    Span::styled(title, header_style),
                    Span::styled(sep, sep_style),
                ]);
                buf.set_line(area.x, y_cursor, &line, area.width);
                y_cursor = y_cursor.saturating_add(1);
            }
            RowEntry::Setting {
                meta_index, key, ..
            } => {
                let Some(meta) = state.registry.all().get(*meta_index) else {
                    continue;
                };
                let value_opt = values.get(row_pos).and_then(|v| v.as_ref());
                let is_selected = row_idx == state.selected;
                let is_expanded = expanded_snapshot.contains(key);

                // Group rows carry no scalar value; render a chevron row that
                // opens the sub-sheet (skips the value/edited machinery below).
                if matches!(meta.kind, SettingKind::Group { .. }) {
                    let is_hovered = hover_row_snapshot == Some(row_idx);
                    let value_rect = render_setting_group_row(
                        buf,
                        label_rect,
                        meta,
                        is_selected,
                        is_hovered,
                        is_expanded,
                        theme,
                    );
                    state.value_hit_rects[row_idx] = value_rect;
                    y_cursor = y_cursor.saturating_add(1);
                    // Mirror normal rows: render the description inline when the
                    // group's key is expanded (Right/l). The group has no value,
                    // so this is the only place its description can surface.
                    if is_expanded && y_cursor < area_end {
                        let desc_height = area_end - y_cursor;
                        let desc_rect = Rect {
                            x: area.x,
                            y: y_cursor,
                            width: area.width,
                            height: desc_height.min(8),
                        };
                        render_expanded_description(buf, desc_rect, meta, theme);
                        let consumed =
                            wrapped_description_height(meta, area.width, desc_rect.height);
                        y_cursor = y_cursor.saturating_add(consumed);
                    }
                    continue;
                }

                let value = match value_opt {
                    Some(v) => v,
                    None => {
                        render_setting_row_no_value(
                            buf,
                            label_rect,
                            meta,
                            max_label_w,
                            is_selected,
                            theme,
                        );
                        y_cursor = y_cursor.saturating_add(1);
                        continue;
                    }
                };

                // Decide 1 vs 2 line layout; fall back to 1 if viewport is tight.
                let value_display = match value {
                    SettingValue::Bool(b) => {
                        if *b {
                            "on".to_string()
                        } else {
                            "off".to_string()
                        }
                    }
                    SettingValue::String(s) => {
                        if s.is_empty() && matches!(meta.kind, SettingKind::DynamicEnum { .. }) {
                            "(no override)".to_string()
                        } else {
                            s.clone()
                        }
                    }
                    SettingValue::Enum(e) => display_for_enum_canonical(&meta.kind, e).to_string(),
                    SettingValue::Int(i) => i.to_string(),
                };
                let show_restart_pill_for_layout = meta.restart_required && is_expanded;
                let layout_decision = row_layout(
                    area.width,
                    meta.label,
                    &value_display,
                    show_restart_pill_for_layout,
                );
                let want_two_lines = !matches!(layout_decision, RowLayout::OneLine);
                // Only allocate 2 lines if the viewport has room.
                let row_height: u16 = if want_two_lines && y_cursor.saturating_add(2) <= area_end {
                    2
                } else {
                    1
                };

                let render_area = Rect {
                    x: area.x,
                    y: y_cursor,
                    width: area.width,
                    height: row_height,
                };
                // Hit-rect spans both lines for two-line rows.
                state.row_rects[row_idx] = render_area;

                let is_hovered = hover_row_snapshot == Some(row_idx);
                let value_rect = render_setting_row(
                    buf,
                    render_area,
                    meta,
                    value,
                    max_label_w,
                    is_selected,
                    theme,
                    is_expanded,
                    is_hovered,
                );
                state.value_hit_rects[row_idx] = value_rect;
                y_cursor = y_cursor.saturating_add(row_height);

                if is_expanded && y_cursor < area_end {
                    let desc_height = area_end - y_cursor;
                    let desc_rect = Rect {
                        x: area.x,
                        y: y_cursor,
                        width: area.width,
                        height: desc_height.min(8), // cap at 8 lines per row to keep scroll sane
                    };
                    render_expanded_description(buf, desc_rect, meta, theme);
                    // Re-measure how many lines the wrapped description
                    // actually consumed, so y_cursor advances precisely.
                    let consumed = wrapped_description_height(meta, area.width, desc_rect.height);
                    y_cursor = y_cursor.saturating_add(consumed);
                }
            }
        }
    }
}

/// Compute the minimum scroll_offset that keeps filtered position
/// `fpos` visible within `visible_h` lines. Walks backward,
/// accounting for variable row heights and header gaps.
fn compute_min_scroll_offset_for_visibility(
    filtered_cache: &[usize],
    rows: &[RowEntry],
    row_heights: &[u16],
    fpos: usize,
    visible_h: usize,
) -> usize {
    if visible_h == 0 || fpos >= filtered_cache.len() {
        return fpos;
    }
    // Visual lines consumed so far. `fpos` itself sits at the top of
    // the viewport, so it doesn't earn a blank-above-header even if
    // it IS a header.
    let fpos_height = row_heights.get(fpos).copied().unwrap_or(1) as usize;
    let mut lines_used: usize = fpos_height;
    if lines_used > visible_h {
        // Even the focused row alone doesn't fit; clamp to fpos so
        // the down-stream renderer at least shows its label.
        return fpos;
    }
    let mut offset = fpos;
    while offset > 0 {
        let candidate = offset - 1;
        let candidate_height = row_heights.get(candidate).copied().unwrap_or(1) as usize;
        // Cost of including `candidate` as the new top of the
        // viewport: its own visual height, plus 1 line for the
        // blank-above-header that the OLD top (`offset`) now
        // earns (since it's no longer the first row rendered).
        let old_first_idx = filtered_cache[offset];
        let old_first_is_header = matches!(rows[old_first_idx], RowEntry::Header { .. });
        let cost: usize = candidate_height + usize::from(old_first_is_header);
        if lines_used.saturating_add(cost) > visible_h {
            break;
        }
        lines_used += cost;
        offset = candidate;
    }
    offset
}

/// Precompute the visual height (in terminal rows) of each entry in
/// `state.filtered_cache`, using the same `row_layout` /
/// `wrapped_description_height` math the forward render loop uses.
///
/// The cost passed to [`compute_min_scroll_offset_for_visibility`]
/// is the row's intrinsic height EXCLUDING the blank-line-above-
/// header gap — that gap is accounted for inside the scroll helper's
/// backward walk because it depends on the runtime position relative
/// to the viewport top.
///
/// Cost: O(visible filtered rows) per render, bounded by the
/// registry size (~15 entries today). Each row does at most one
/// `word_wrap_line` call (for expanded descriptions). Allocations
/// are confined to a single `Vec<u16>` per call; per-row layout
/// math is on the stack.
fn compute_filtered_row_heights(state: &SettingsModalState, area_width: u16) -> Vec<u16> {
    let mut heights = Vec::with_capacity(state.filtered_cache.len());
    for &row_idx in &state.filtered_cache {
        let Some(row) = state.rows.get(row_idx) else {
            heights.push(1);
            continue;
        };
        match row {
            RowEntry::Header { .. } => heights.push(1),
            RowEntry::Setting {
                meta_index, key, ..
            } => {
                let Some(meta) = state.registry.all().get(*meta_index) else {
                    heights.push(1);
                    continue;
                };
                // Group rows carry no value; height = chevron row + the expanded
                // description (cap 8), agreeing with the forward render loop.
                if matches!(meta.kind, SettingKind::Group { .. }) {
                    let mut h: u16 = 1;
                    if state.expanded_keys.contains(key) {
                        h = h.saturating_add(wrapped_description_height(meta, area_width, 8));
                    }
                    heights.push(h);
                    continue;
                }
                let Some(value) = state.value_for(key) else {
                    heights.push(1);
                    continue;
                };
                let is_expanded = state.expanded_keys.contains(key);
                let value_display = match &value {
                    SettingValue::Bool(b) => {
                        if *b {
                            "on".to_string()
                        } else {
                            "off".to_string()
                        }
                    }
                    SettingValue::String(s) => {
                        if s.is_empty() && matches!(meta.kind, SettingKind::DynamicEnum { .. }) {
                            "(no override)".to_string()
                        } else {
                            s.clone()
                        }
                    }
                    SettingValue::Enum(e) => display_for_enum_canonical(&meta.kind, e).to_string(),
                    SettingValue::Int(i) => i.to_string(),
                };
                let show_restart_pill = meta.restart_required && is_expanded;
                let layout = row_layout(area_width, meta.label, &value_display, show_restart_pill);
                let mut h: u16 = match layout {
                    RowLayout::OneLine => 1,
                    RowLayout::TwoLine | RowLayout::TwoLineWithLabelTruncation => 2,
                };
                if is_expanded {
                    // Cap matches the forward render loop at line
                    // 2040 (`desc_rect.height = ... .min(8)`).
                    h = h.saturating_add(wrapped_description_height(meta, area_width, 8));
                }
                heights.push(h);
            }
        }
    }
    heights
}

/// Wrapped description height for scroll math (mirrors render path).
fn wrapped_description_height(meta: &SettingMeta, area_width: u16, cap: u16) -> u16 {
    let indent = 4u16.min(area_width);
    let wrap_w = area_width.saturating_sub(indent);
    if wrap_w == 0 {
        return 0;
    }
    let line = Line::from(Span::raw(meta.description));
    let wrapped = crate::render::wrapping::word_wrap_line(&line, wrap_w as usize);
    (wrapped.len() as u16).min(cap)
}

// Picker prefix constants (hoisted to avoid per-frame allocation).
const PICKER_PREFIX_FOCUSED: &str = " \u{25CF}  ";
const PICKER_PREFIX_UNFOCUSED: &str = " \u{25CB}  ";

const PICKER_PREFIX_W: u16 = 4;
const PICKER_SEPARATOR: &str = " \u{00B7} ";
const PICKER_SEPARATOR_W: u16 = 3;

const PICKER_MARKER_W: u16 = 1;
/// Soft product cap on static Enum choices (settings unit tests enforce it).
///
/// The chooser already scrolls within the viewport when the focused choice
/// falls off-screen (`picker_scroll_offset`); this limit exists so catalogs
/// stay intentionally curated rather than unbounded. Sized to fit the full
/// Grok STT language list (25 codes + client-only `auto` = 26) with headroom.
pub(crate) const MAX_PICKER_CHOICES: usize = 32;

/// Render the shared sub-pane header (bold title row + word-wrapped description)
/// used by all four sub-pane renderers — the enum chooser, the group sub-sheet,
/// and the string/int editors. Returns `header_rows`: the rows consumed (title +
/// optional description + the 1-row gap), so the caller positions its body at
/// `area.y + header_rows`. The bodies differ and stay in each caller.
///
/// `min_non_desc_rows` is the vertical budget (excluding the description rows
/// themselves) that must fit before the description renders at all: `2` for the
/// choosers (title + gap), `3` for the editors (title + gap + the input/stepper
/// row). Callers keep their own `if area.height <= header_rows { return; }`.
fn render_sub_pane_header(
    buf: &mut Buffer,
    area: Rect,
    theme: &Theme,
    title: &str,
    description: &str,
    min_non_desc_rows: u16,
) -> u16 {
    // ── Row 0: title (truncated with `…`). ────────────────────────
    let title_style = Style::default()
        .fg(theme.text_primary)
        .bg(theme.bg_base)
        .add_modifier(Modifier::BOLD);
    let title_text: std::borrow::Cow<'_, str> = if title.width() <= area.width as usize {
        std::borrow::Cow::Borrowed(title)
    } else {
        std::borrow::Cow::Owned(truncate_str(title, area.width as usize))
    };
    let title_w = (title_text.width() as u16).min(area.width);
    buf.set_span(
        area.x,
        area.y,
        &Span::styled(title_text.as_ref(), title_style),
        title_w,
    );

    // ── Row 1+: word-wrapped description ──────────────────────────
    let description_wrapped = wrap_description(description, area.width);
    let desc_rows: u16 = description_wrapped.len() as u16;
    let has_description =
        desc_rows > 0 && area.height >= min_non_desc_rows.saturating_add(desc_rows);
    if has_description {
        let desc_style = Style::default().fg(theme.gray_dim).bg(theme.bg_base);
        for (i, wrap_line) in description_wrapped.iter().enumerate() {
            let y = area.y + 1 + i as u16;
            if y >= area.y + area.height {
                break;
            }
            let w = (wrap_line.width() as u16).min(area.width);
            buf.set_span(area.x, y, &Span::styled(wrap_line.as_str(), desc_style), w);
        }
    }

    if has_description { 2 + desc_rows } else { 2 }
}

/// Render the Enum chooser sub-pane. Title + description + radio-style
/// choice list with scrolling and `… N more` overflow indicator.
fn render_picking_enum(buf: &mut Buffer, area: Rect, state: &SettingsModalState, theme: &Theme) {
    debug_assert_eq!(
        PICKER_PREFIX_FOCUSED.width(),
        PICKER_PREFIX_W as usize,
        "PICKER_PREFIX_W drifted from PICKER_PREFIX_FOCUSED width",
    );
    debug_assert_eq!(
        PICKER_PREFIX_UNFOCUSED.width(),
        PICKER_PREFIX_W as usize,
        "PICKER_PREFIX_W drifted from PICKER_PREFIX_UNFOCUSED width",
    );
    debug_assert_eq!(
        PICKER_SEPARATOR.width(),
        PICKER_SEPARATOR_W as usize,
        "PICKER_SEPARATOR_W drifted from PICKER_SEPARATOR width",
    );

    let (setting_key, choices_idx) = match &state.mode {
        SettingsModalMode::PickingEnum {
            key, choices_idx, ..
        } => (*key, *choices_idx),
        _ => return,
    };
    let Some(meta) = state.registry.find(setting_key) else {
        return;
    };

    let choices: Vec<OwnedEnumChoice> = match &meta.kind {
        SettingKind::Enum { choices, .. } => {
            effective_enum_choices(setting_key, choices, &state.pager_snapshot)
                .into_iter()
                .map(|c| OwnedEnumChoice {
                    canonical: c.canonical.to_string(),
                    display: c.display.to_string(),
                    description: c.description.to_string(),
                })
                .collect()
        }
        SettingKind::DynamicEnum { source, .. } => {
            dynamic_enum_choices(*source, &state.pager_snapshot)
        }
        _ => return,
    };

    if area.width == 0 || area.height == 0 {
        return;
    }

    // Choosers need title + gap (2) before the description renders.
    let header_rows = render_sub_pane_header(buf, area, theme, meta.label, meta.description, 2);
    if area.height <= header_rows {
        return;
    }
    let choices_y = area.y + header_rows;
    let max_choices_h = area.height.saturating_sub(header_rows) as usize;
    if max_choices_h == 0 {
        return;
    }

    // ── Per-choice wrapped layout ─────────────────────────────────
    let layouts: Vec<PickerChoiceLayout> = choices
        .iter()
        .map(|choice| compute_picker_choice_layout(choice, area.width))
        .collect();
    let total_h: u16 = layouts.iter().map(|l| l.height).sum();

    // ── Scroll offset (variable per-choice height) ────────────────
    let needs_overflow = total_h as usize > max_choices_h;
    let available_h: u16 = if needs_overflow {
        (max_choices_h as u16).saturating_sub(1).max(1)
    } else {
        max_choices_h as u16
    };
    let scroll_offset = picker_scroll_offset(&layouts, choices_idx, available_h);

    let mut visible_end = scroll_offset;
    let mut consumed_h: u16 = 0;
    for (i, layout) in layouts.iter().enumerate().skip(scroll_offset) {
        let next = consumed_h.saturating_add(layout.height);
        if next > available_h {
            break;
        }
        consumed_h = next;
        visible_end = i + 1;
    }
    // Defensive: always show the focused choice even if it's clipped.
    if visible_end <= choices_idx {
        visible_end = choices_idx + 1;
    }
    let _ = consumed_h; // height bookkeeping kept for future tuning

    // ── Hit-rect bookkeeping ──────────────────────────────────────
    let mut picker_choice_rects: Vec<Rect> = vec![Rect::default(); choices.len()];

    // ── Choice rows ───────────────────────────────────────────────
    let bg_focused = theme.bg_visual;
    let bg_unfocused = theme.bg_base;
    let bg_hovered = theme.bg_hover;
    let fg_primary = theme.text_primary;
    let fg_gray = theme.gray;
    let fg_accent = theme.accent_user;

    let mut y_cursor = choices_y;
    for (choice_i, layout) in layouts
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_end - scroll_offset)
    {
        let choice = &choices[choice_i];
        let is_focused = choice_i == choices_idx;

        let is_hovered = !is_focused && state.hover_row == Some(choice_i);
        let bg = if is_focused {
            bg_focused
        } else if is_hovered {
            bg_hovered
        } else {
            bg_unfocused
        };

        let display_style = if is_focused {
            Style::default()
                .fg(fg_primary)
                .bg(bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(fg_primary).bg(bg)
        };
        let desc_style = Style::default().fg(fg_gray).bg(bg);
        let marker_style = if is_focused {
            Style::default().fg(fg_accent).bg(bg)
        } else {
            Style::default().fg(fg_gray).bg(bg)
        };
        let marker = if is_focused {
            crate::glyphs::filled_dot()
        } else {
            "\u{25CB}"
        };

        let block_rect = Rect {
            x: area.x,
            y: y_cursor,
            width: area.width,
            height: layout.height,
        };
        buf.set_style(block_rect, Style::default().bg(bg));
        picker_choice_rects[choice_i] = block_rect;

        // ── Line 1: prefix + display + (· + first wrap line) ──────
        let y = y_cursor;
        if area.width > 0 {
            // Leading space (col 0 of the row).
            buf.set_span(
                area.x,
                y,
                &Span::styled(" ", display_style),
                1.min(area.width),
            );
        }
        if area.width > 1 {
            // Marker glyph at col 1.
            buf.set_span(
                area.x + 1,
                y,
                &Span::styled(marker, marker_style),
                PICKER_MARKER_W.min(area.width.saturating_sub(1)),
            );
        }
        if area.width > 2 {
            // Trailing two spaces at cols 2-3.
            let pad_w = 2u16.min(area.width.saturating_sub(2));
            buf.set_span(area.x + 2, y, &Span::styled("  ", display_style), pad_w);
        }

        // Display name (truncated).
        let display_x = area.x.saturating_add(PICKER_PREFIX_W);
        let display_room = (area.x + area.width).saturating_sub(display_x) as usize;
        if display_room == 0 {
            y_cursor = y_cursor.saturating_add(layout.height);
            continue;
        }
        let display_text: std::borrow::Cow<'_, str> = if choice.display.width() <= display_room {
            std::borrow::Cow::Borrowed(choice.display.as_str())
        } else {
            std::borrow::Cow::Owned(truncate_str(&choice.display, display_room))
        };
        let display_w =
            (display_text.width() as u16).min(area.width.saturating_sub(PICKER_PREFIX_W));
        buf.set_span(
            display_x,
            y,
            &Span::styled(display_text.as_ref(), display_style),
            display_w,
        );

        let has_choice_desc = !choice.description.trim().is_empty();
        if !has_choice_desc {
            y_cursor = y_cursor.saturating_add(layout.height);
            continue;
        }
        let after_display_x = display_x.saturating_add(display_w);
        let sep_room = (area.x + area.width).saturating_sub(after_display_x);
        if sep_room == 0 {
            y_cursor = y_cursor.saturating_add(layout.height);
            continue;
        }
        let sep_w = PICKER_SEPARATOR_W.min(sep_room);
        buf.set_span(
            after_display_x,
            y,
            &Span::styled(PICKER_SEPARATOR, desc_style),
            sep_w,
        );

        let desc_x = after_display_x + sep_w;
        let desc_room = (area.x + area.width).saturating_sub(desc_x) as usize;
        if desc_room == 0 {
            y_cursor = y_cursor.saturating_add(layout.height);
            continue;
        }

        // Narrow fallback: truncate on one line if wrapping fails.
        if layout.wrap_lines.is_empty() {
            let desc_text: std::borrow::Cow<'_, str> = if choice.description.width() <= desc_room {
                std::borrow::Cow::Borrowed(choice.description.as_str())
            } else {
                std::borrow::Cow::Owned(truncate_str(&choice.description, desc_room))
            };
            let desc_w = (desc_text.width() as u16).min(area.x + area.width - desc_x);
            buf.set_span(
                desc_x,
                y,
                &Span::styled(desc_text.as_ref(), desc_style),
                desc_w,
            );
            y_cursor = y_cursor.saturating_add(layout.height);
            continue;
        }

        // Line 1: first wrap line at the description column.
        let first_line = &layout.wrap_lines[0];
        let first_w = (first_line.width() as u16).min(area.x + area.width - desc_x);
        buf.set_span(
            desc_x,
            y,
            &Span::styled(first_line.as_str(), desc_style),
            first_w,
        );

        // Lines 2..N: continuation lines aligned under first_line.
        for (cont_i, wrap_line) in layout.wrap_lines.iter().enumerate().skip(1) {
            let cont_y = y + cont_i as u16;
            if cont_y >= area.y + area.height {
                break;
            }
            let cont_w = (wrap_line.width() as u16).min(area.x + area.width - desc_x);
            buf.set_span(
                desc_x,
                cont_y,
                &Span::styled(wrap_line.as_str(), desc_style),
                cont_w,
            );
        }

        y_cursor = y_cursor.saturating_add(layout.height);
    }

    // ── Overflow indicator: "… N more" on the row right below the
    //    last rendered choice. ─────────────────────────────────────
    if needs_overflow && visible_end < choices.len() {
        let more_count = choices.len() - visible_end;
        let overflow_y = y_cursor;
        if overflow_y < choices_y + max_choices_h as u16 && overflow_y < area.y + area.height {
            let overflow_style = Style::default().fg(theme.gray_dim).bg(theme.bg_base);
            let raw = format!("\u{2026} {more_count} more");
            let overflow_text: std::borrow::Cow<'_, str> = if raw.width() <= area.width as usize {
                std::borrow::Cow::Owned(raw)
            } else {
                std::borrow::Cow::Owned(truncate_str(&raw, area.width as usize))
            };
            let overflow_w = (overflow_text.width() as u16).min(area.width);
            let overflow_rect = Rect {
                x: area.x,
                y: overflow_y,
                width: area.width,
                height: 1,
            };
            buf.set_style(overflow_rect, Style::default().bg(theme.bg_base));
            buf.set_span(
                area.x,
                overflow_y,
                &Span::styled(overflow_text.as_ref(), overflow_style),
                overflow_w,
            );
        }
    }

    PICKER_RECTS_SCRATCH.with(|cell| {
        *cell.borrow_mut() = picker_choice_rects;
    });
    let _ = total_h; // suppress unused-var warning on some builds
}

// Thread-local scratch to ferry hit-rects out of `render_picking_enum`
// (which takes `&state`) into `state.picker_choice_rects`.
thread_local! {
    static PICKER_RECTS_SCRATCH: std::cell::RefCell<Vec<Rect>>
        = const { std::cell::RefCell::new(Vec::new()) };
}

/// Read-and-clear the most recent per-choice hit-rects produced by
/// `render_picking_enum`. Returns an empty Vec when called before
/// the first picker render (or after a non-picker frame reset the
/// scratch).
pub(crate) fn take_picker_choice_rects() -> Vec<Rect> {
    PICKER_RECTS_SCRATCH.with(|cell| std::mem::take(&mut *cell.borrow_mut()))
}

/// Render the group sub-sheet: title + description + one row per child Bool
/// toggle (`<marker> <Label> … <on/off>`). Returns the per-child hit-rects
/// (parallel to the group's children) for mouse routing. Mirrors the enum
/// chooser's title/description/list shape but for independent toggles.
fn render_picking_group(
    buf: &mut Buffer,
    area: Rect,
    state: &SettingsModalState,
    theme: &Theme,
) -> Vec<Rect> {
    let (group_key, child_idx) = match &state.mode {
        SettingsModalMode::PickingGroup { key, child_idx } => (*key, *child_idx),
        _ => return Vec::new(),
    };
    let Some(group_meta) = state.registry.find(group_key) else {
        return Vec::new();
    };
    let children = group_children(state, group_key);
    if area.width == 0 || area.height == 0 {
        return Vec::new();
    }

    // Chooser shape: title + gap (2) before the description renders.
    let header_rows = render_sub_pane_header(
        buf,
        area,
        theme,
        group_meta.label,
        group_meta.description,
        2,
    );
    if area.height <= header_rows {
        return Vec::new();
    }
    let mut y = area.y + header_rows;
    let area_end = area.y + area.height;

    // ── Child toggle rows. ────────────────────────────────────────
    let mut rects: Vec<Rect> = vec![Rect::default(); children.len()];
    for (i, child_key) in children.iter().enumerate() {
        if y >= area_end {
            break;
        }
        let Some(child_meta) = state.registry.find(child_key) else {
            continue;
        };
        let is_focused = i == child_idx;
        let is_hovered = !is_focused && state.hover_row == Some(i);
        let bg = if is_focused {
            theme.bg_visual
        } else if is_hovered {
            theme.bg_hover
        } else {
            theme.bg_base
        };
        let row_rect = Rect {
            x: area.x,
            y,
            width: area.width,
            height: 1,
        };
        buf.set_style(row_rect, Style::default().bg(bg));
        rects[i] = row_rect;

        let marker = if is_focused {
            crate::glyphs::filled_dot()
        } else {
            "\u{25CB}"
        };
        let marker_style = if is_focused {
            Style::default().fg(theme.accent_user).bg(bg)
        } else {
            Style::default().fg(theme.gray).bg(bg)
        };
        let label_style = if is_focused {
            Style::default()
                .fg(theme.text_primary)
                .bg(bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text_primary).bg(bg)
        };

        // Value read live from the snapshot (refreshed after each toggle).
        let on = matches!(state.value_for(child_key), Some(SettingValue::Bool(true)));
        let value_text = if on { "on" } else { "off" };
        let value_style = if on {
            Style::default().fg(theme.accent_user).bg(bg)
        } else {
            Style::default().fg(theme.gray).bg(bg)
        };

        // " <marker>  <label> … <value> " (value right-aligned with a pad).
        buf.set_span(
            area.x,
            y,
            &Span::styled(" ", label_style),
            1.min(area.width),
        );
        if area.width > 1 {
            buf.set_span(
                area.x + 1,
                y,
                &Span::styled(marker, marker_style),
                PICKER_MARKER_W.min(area.width - 1),
            );
        }
        let label_x = area.x.saturating_add(PICKER_PREFIX_W);
        let value_w = value_text.width() as u16;
        let value_x = (area.x + area.width)
            .saturating_sub(value_w + 1)
            .max(label_x);
        if value_x > label_x {
            let label_room = (value_x - label_x).saturating_sub(1) as usize;
            let label_text: std::borrow::Cow<'_, str> = if child_meta.label.width() <= label_room {
                std::borrow::Cow::Borrowed(child_meta.label)
            } else {
                std::borrow::Cow::Owned(truncate_str(child_meta.label, label_room))
            };
            let label_w = (label_text.width() as u16).min((value_x - label_x).saturating_sub(1));
            buf.set_span(
                label_x,
                y,
                &Span::styled(label_text.as_ref(), label_style),
                label_w,
            );
        }
        if value_x + value_w <= area.x + area.width {
            buf.set_span(value_x, y, &Span::styled(value_text, value_style), value_w);
        }
        y = y.saturating_add(1);
    }
    rects
}

/// Layout metadata for one picker choice.
struct PickerChoiceLayout {
    height: u16,
    wrap_lines: Vec<String>,
}

/// Compute layout for one picker choice (height + wrapped desc lines).
fn compute_picker_choice_layout(choice: &OwnedEnumChoice, area_width: u16) -> PickerChoiceLayout {
    // No description → 1 line, symbol + display only.
    if choice.description.trim().is_empty() {
        return PickerChoiceLayout {
            height: 1,
            wrap_lines: Vec::new(),
        };
    }

    // Compute the desc column = PICKER_PREFIX_W + display_width + PICKER_SEPARATOR_W.
    // Display gets truncated if it'd overflow; mirror that for layout math.
    let display_room = (area_width as usize).saturating_sub(PICKER_PREFIX_W as usize);
    let display_w = choice.display.width().min(display_room) as u16;
    let after_display = PICKER_PREFIX_W.saturating_add(display_w);
    let after_sep = after_display.saturating_add(PICKER_SEPARATOR_W);

    if after_sep >= area_width {
        // No room for description — single-line fallback.
        return PickerChoiceLayout {
            height: 1,
            wrap_lines: Vec::new(),
        };
    }

    let desc_width = area_width.saturating_sub(after_sep) as usize;
    if desc_width == 0 {
        return PickerChoiceLayout {
            height: 1,
            wrap_lines: Vec::new(),
        };
    }

    let line = Line::from(Span::raw(choice.description.as_str()));
    let wrapped = crate::render::wrapping::word_wrap_line(&line, desc_width);

    let wrap_lines: Vec<String> = wrapped
        .into_iter()
        .map(|l| {
            l.spans
                .into_iter()
                .map(|s| s.content.into_owned())
                .collect::<String>()
        })
        .collect();

    // Defensive: treat empty wrap result as no-wrap.
    if wrap_lines.is_empty() {
        return PickerChoiceLayout {
            height: 1,
            wrap_lines: Vec::new(),
        };
    }

    PickerChoiceLayout {
        height: wrap_lines.len() as u16,
        wrap_lines,
    }
}

/// Smallest scroll offset that keeps the focused choice fully visible.
fn picker_scroll_offset(
    layouts: &[PickerChoiceLayout],
    choices_idx: usize,
    available_h: u16,
) -> usize {
    if layouts.is_empty() || available_h == 0 {
        return 0;
    }
    let n = layouts.len();
    let mut offset = 0usize;
    loop {
        // Sum heights starting at `offset` until adding the next
        // choice would exceed `available_h`.
        let mut consumed: u16 = 0;
        let mut last_visible = offset;
        for (i, layout) in layouts.iter().enumerate().skip(offset) {
            let next = consumed.saturating_add(layout.height);
            if next > available_h {
                break;
            }
            consumed = next;
            last_visible = i + 1;
        }
        // `last_visible` is the exclusive upper bound. If
        // `choices_idx` is in `[offset, last_visible)`, this offset
        // works.
        if choices_idx < last_visible {
            return offset;
        }
        // Otherwise advance. Stop when we've exhausted the list
        // (defensive — shouldn't happen since `choices_idx < n`).
        if offset + 1 >= n {
            return offset;
        }
        offset += 1;
    }
}

/// Min width to draw the Int stepper's `‹`/`›` adornments.
const INT_STEPPER_ADORNMENT_MIN_WIDTH: u16 = 8;

/// Wide-range Int stepper defaults (span > 100): Up/Down small, Left/Right large.
const INT_STEPPER_WIDE_SMALL_STEP: i64 = 5;
const INT_STEPPER_WIDE_LARGE_STEP: i64 = 10;

/// Derive (small, large) step sizes from an Int setting's `[min, max]` span.
/// Narrow dials use unit fine-steps so every in-range value is reachable;
/// wide ranges keep the original ±5 / ±10 feel.
fn int_step_sizes(min: i64, max: i64) -> (i64, i64) {
    let span = max.saturating_sub(min).max(0);
    if span <= 20 {
        // scroll_lines 1..=10 (span 9): unit steps on both small and large.
        (1, (span / 5).max(1))
    } else if span <= 100 {
        // scroll_speed 1..=100 (span 99): unit fine, ±5 coarse.
        (1, 5)
    } else {
        // max_thoughts_width 40..=500 (span 460).
        (INT_STEPPER_WIDE_SMALL_STEP, INT_STEPPER_WIDE_LARGE_STEP)
    }
}

/// Footer labels for the Int stepper (must be `'static` for `Shortcut`).
fn int_step_footer_labels(min: i64, max: i64) -> (&'static str, &'static str) {
    let (small, large) = int_step_sizes(min, max);
    match (small, large) {
        (1, 1) => ("\u{2191}/\u{2193} +/-1", "\u{2190}/\u{2192} +/-1"),
        (1, 5) => ("\u{2191}/\u{2193} +/-1", "\u{2190}/\u{2192} +/-5"),
        (5, 10) => ("\u{2191}/\u{2193} +/-5", "\u{2190}/\u{2192} +/-10"),
        // Defensive fallback if thresholds change without new static pairs.
        (1, _) => ("\u{2191}/\u{2193} +/-1", "\u{2190}/\u{2192} step"),
        (5, _) => ("\u{2191}/\u{2193} +/-5", "\u{2190}/\u{2192} step"),
        _ => ("\u{2191}/\u{2193} step", "\u{2190}/\u{2192} step"),
    }
}

// ‹ / › (U+2039 / U+203A) — fall back to ASCII `<` / `>` on legacy ConHost.
fn int_stepper_left_glyph() -> &'static str {
    crate::glyphs::chevron_left()
}

fn int_stepper_right_glyph() -> &'static str {
    crate::glyphs::chevron()
}

/// Sample text for the `max_thoughts_width` live wrap preview.
const MAX_THOUGHTS_WIDTH_PREVIEW_SAMPLE: &str = "Let me trace through the call sites. First, \
    I'll need to look at how the dispatch flow handles the new variant. Then I'll verify the \
    rollback path preserves the previous state correctly.";

/// Min width to render the wrap preview.
const MAX_THOUGHTS_WIDTH_PREVIEW_MIN_WIDTH: u16 = 30;

/// Min remaining height to render the wrap preview below the stepper.
const MAX_THOUGHTS_WIDTH_PREVIEW_MIN_HEIGHT: u16 = 5;

/// Render the inline editor. Int settings use a stepper; String
/// settings use a text input with cursor and validation feedback.
fn render_editing_value(
    buf: &mut Buffer,
    area: Rect,
    state: &mut SettingsModalState,
    theme: &Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    state.editor_adornment_rects = (Rect::default(), Rect::default());

    // Snapshot mode payload to avoid borrow conflicts with mut state.
    let (setting_key, buffer_owned, cursor_byte, validation_error_owned, kind_is_int) = {
        let (setting_key, buffer, cursor_byte, validation_error) = match &state.mode {
            SettingsModalMode::EditingValue {
                key,
                buffer,
                cursor_byte,
                validation_error,
            } => (*key, buffer.clone(), *cursor_byte, validation_error.clone()),
            _ => return,
        };
        let kind_is_int = state
            .registry
            .find(setting_key)
            .map(|m| matches!(m.kind, SettingKind::Int { .. }))
            .unwrap_or(false);
        (
            setting_key,
            buffer,
            cursor_byte,
            validation_error,
            kind_is_int,
        )
    };

    if kind_is_int {
        let Some(meta) = state.registry.find(setting_key) else {
            return;
        };
        // Snapshot meta fields to release registry borrow.
        let label = meta.label;
        let description = meta.description;
        render_int_stepper(
            buf,
            area,
            state,
            setting_key,
            label,
            description,
            &buffer_owned,
            theme,
        );
        return;
    }

    let buffer = buffer_owned.as_str();
    let validation_error = validation_error_owned.as_deref();
    let Some(meta) = state.registry.find(setting_key) else {
        return;
    };

    // Editors reserve title + gap + the input row (3) before the description.
    let header_rows = render_sub_pane_header(buf, area, theme, meta.label, meta.description, 3);
    if area.height <= header_rows {
        return;
    }
    let input_y = area.y + header_rows;

    // ── Row 3: input line. ────────────────────────────────────────
    let has_error = validation_error.is_some();
    let input_bg = theme.bg_visual;
    let input_fg = if has_error {
        theme.accent_error
    } else {
        theme.text_primary
    };
    let cursor_style = Style::default().fg(theme.accent_user).bg(input_bg);
    let input_style = Style::default().fg(input_fg).bg(input_bg);

    let input_row_rect = Rect {
        x: area.x,
        y: input_y,
        width: area.width,
        height: 1,
    };
    buf.set_style(input_row_rect, Style::default().bg(theme.bg_base));

    let input_x = area.x;
    let buffer_room_end_x = area.x + area.width;
    let buffer_room = buffer_room_end_x.saturating_sub(input_x) as usize;
    if buffer_room == 0 {
        return; // No room to render the buffer.
    }

    let input_strip_rect = Rect {
        x: input_x,
        y: input_y,
        width: buffer_room as u16,
        height: 1,
    };
    buf.set_style(input_strip_rect, Style::default().bg(input_bg));

    let cursor_reserve = 1usize;
    let visible_buffer_w = buffer_room.saturating_sub(cursor_reserve);

    // Empty-buffer placeholder.
    if buffer.is_empty() {
        let placeholder = match &meta.kind {
            SettingKind::String { validator, .. } => match validator {
                StringValidator::KnownModel => "<empty — use shell default>",
                StringValidator::NonEmptyToken => "<type a value>",
                StringValidator::Any => "<type a value>",
            },
            _ => "",
        };
        if !placeholder.is_empty() && visible_buffer_w > 0 {
            let placeholder_text: std::borrow::Cow<'_, str> =
                if placeholder.width() <= visible_buffer_w {
                    std::borrow::Cow::Borrowed(placeholder)
                } else {
                    std::borrow::Cow::Owned(truncate_str(placeholder, visible_buffer_w))
                };
            let placeholder_w = (placeholder_text.width() as u16).min(visible_buffer_w as u16);
            let placeholder_style = Style::default().fg(theme.gray_dim).bg(input_bg);
            buf.set_span(
                input_x,
                input_y,
                &Span::styled(placeholder_text.as_ref(), placeholder_style),
                placeholder_w,
            );
        }
        // Render the cursor at the start.
        let cursor_x = input_x;
        buf.set_span(
            cursor_x,
            input_y,
            &Span::styled(crate::glyphs::selection_bar(), cursor_style),
            1,
        );
    } else {
        // Cursor-following pan.
        let cursor_col = buffer[..cursor_byte.min(buffer.len())].width();
        let buffer_w = buffer.width();
        let view_offset = if buffer_w <= visible_buffer_w {
            0
        } else if cursor_col >= visible_buffer_w {
            cursor_col + 1 - visible_buffer_w
        } else {
            // Cursor fits within the first window; no scroll.
            0
        };

        let start_byte = if view_offset == 0 {
            0
        } else {
            let mut acc = 0usize;
            buffer
                .char_indices()
                .find_map(|(idx, ch)| {
                    if acc >= view_offset {
                        Some(idx)
                    } else {
                        acc += unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                        None
                    }
                })
                .unwrap_or(buffer.len())
        };

        // Render the visible tail.
        let tail = &buffer[start_byte..];
        // The visible portion may still be wider than the room when
        // a wide grapheme straddles the right boundary; cap with
        // `truncate_str` defensively.
        let tail_text: std::borrow::Cow<'_, str> = if tail.width() <= visible_buffer_w {
            std::borrow::Cow::Borrowed(tail)
        } else {
            std::borrow::Cow::Owned(truncate_str(tail, visible_buffer_w))
        };
        let tail_w = (tail_text.width() as u16).min(visible_buffer_w as u16);
        buf.set_span(
            input_x,
            input_y,
            &Span::styled(tail_text.as_ref(), input_style),
            tail_w,
        );

        // Cursor lands at the logical column relative to the view.
        let cursor_visual_col = cursor_col.saturating_sub(view_offset);
        let cursor_x = input_x + (cursor_visual_col as u16).min(buffer_room as u16 - 1);
        buf.set_span(
            cursor_x,
            input_y,
            &Span::styled(crate::glyphs::selection_bar(), cursor_style),
            1,
        );
    }

    // ── Row 4: validation error. ──────────────────────────────────
    if area.height > header_rows + 1
        && let Some(err) = validation_error
    {
        let err_y = input_y + 1;
        let err_style = Style::default().fg(theme.accent_error).bg(theme.bg_base);
        let err_text: std::borrow::Cow<'_, str> = if err.width() <= area.width as usize {
            std::borrow::Cow::Borrowed(err)
        } else {
            std::borrow::Cow::Owned(truncate_str(err, area.width as usize))
        };
        let err_w = (err_text.width() as u16).min(area.width);
        buf.set_span(
            area.x,
            err_y,
            &Span::styled(err_text.as_ref(), err_style),
            err_w,
        );
    }
}

/// Render the Int stepper: title + description + centered `‹ N ›`.
/// Populates `editor_adornment_rects` for mouse click targets.
#[allow(clippy::too_many_arguments)]
fn render_int_stepper(
    buf: &mut Buffer,
    area: Rect,
    state: &mut SettingsModalState,
    setting_key: SettingKey,
    label: &'static str,
    description: &'static str,
    buffer: &str,
    theme: &Theme,
) {
    // Editors reserve title + gap + the stepper row (3) before the description.
    let header_rows = render_sub_pane_header(buf, area, theme, label, description, 3);
    if area.height <= header_rows {
        return;
    }
    let stepper_y = area.y + header_rows;

    // ── Row 3: centered stepper "‹  N  ›". ────────────────────────
    let value_text = if buffer.is_empty() {
        // Defensive — try_enter_editing_value seeds buffer from the
        // current value, so this branch should be unreachable, but
        // a blank cell would be confusing if a future refactor
        // dropped the seed.
        "—".to_string()
    } else {
        buffer.to_string()
    };
    let value_style = Style::default()
        .fg(theme.accent_user)
        .bg(theme.bg_base)
        .add_modifier(Modifier::BOLD);
    let arrow_style = Style::default().fg(theme.accent_user).bg(theme.bg_base);

    let left_w = int_stepper_left_glyph().width() as u16;
    let right_w = int_stepper_right_glyph().width() as u16;
    let value_w = value_text.width() as u16;
    // Layout: "‹  N  ›" — 2 cells between each glyph for breathing
    // room. Total width = left + 2 + value + 2 + right.
    let inter_pad: u16 = 2;
    let total_w = left_w + inter_pad + value_w + inter_pad + right_w;
    let render_arrows = area.width >= INT_STEPPER_ADORNMENT_MIN_WIDTH;

    if render_arrows && total_w <= area.width {
        // Center the full layout.
        let stepper_x = area.x + (area.width - total_w) / 2;
        let left_x = stepper_x;
        let value_x = left_x + left_w + inter_pad;
        let right_x = value_x + value_w + inter_pad;

        buf.set_span(
            left_x,
            stepper_y,
            &Span::styled(int_stepper_left_glyph(), arrow_style),
            left_w,
        );
        buf.set_span(
            value_x,
            stepper_y,
            &Span::styled(value_text.as_str(), value_style),
            value_w,
        );
        buf.set_span(
            right_x,
            stepper_y,
            &Span::styled(int_stepper_right_glyph(), arrow_style),
            right_w,
        );

        state.editor_adornment_rects = (
            Rect {
                x: left_x,
                y: stepper_y,
                width: left_w,
                height: 1,
            },
            Rect {
                x: right_x,
                y: stepper_y,
                width: right_w,
                height: 1,
            },
        );
    } else {
        // Too narrow for arrows — render the value alone, centered.
        let v_w = value_w.min(area.width);
        let value_x = area.x + (area.width - v_w) / 2;
        buf.set_span(
            value_x,
            stepper_y,
            &Span::styled(value_text.as_str(), value_style),
            v_w,
        );
    }

    // **In-pane hint dropped.** Earlier revisions
    // rendered a centered `↑/↓ +/-5   ←/→ +/-10   Enter commit · Esc
    // cancel` strip here, but the chrome footer's
    // `build_int_editor_shortcuts` already exposes the same content
    // at the bottom of the modal. On tall viewports both rendered
    // simultaneously — same keys, different separator (`·` vs `|`),
    // duplicate visual noise. We rely on the chrome footer alone
    // now; if the chrome ever fails to render its shortcut row (a
    // future regression), the user can still discover the keys via
    // the shortcuts cheatsheet (`?`).

    // ── Live wrap preview for max_thoughts_width. ─────────────────
    //
    // When the user is stepping `max_thoughts_width`, render a
    // sample thinking-text preview directly below the stepper that
    // wraps live at the current pending value. The preview sits in
    // the rows immediately after the stepper (1 blank row + title +
    // N content rows); any rows below the last content row of the
    // preview stay blank — the chrome footer sits below
    // `inner_area`, not inside `area`.
    //
    // Gated on `setting_key == MAX_THOUGHTS_WIDTH_KEY` so future
    // Int settings don't inherit the preview behavior implicitly.
    // The string equality is sufficient because key uniqueness is
    // enforced at registry-load time (see
    // `SettingsRegistry::defaults` / `::from_entries`).
    if setting_key == crate::settings::defs::MAX_THOUGHTS_WIDTH_KEY {
        let stepper_end_y = stepper_y.saturating_add(1);
        let area_end_y = area.y.saturating_add(area.height);
        let preview_h = area_end_y.saturating_sub(stepper_end_y);
        if preview_h >= MAX_THOUGHTS_WIDTH_PREVIEW_MIN_HEIGHT {
            let preview_area = Rect {
                x: area.x,
                y: stepper_end_y,
                width: area.width,
                height: preview_h,
            };
            let pending_value = parse_max_thoughts_width_buffer(buffer);
            render_max_thoughts_width_preview(buf, preview_area, pending_value, theme);
        }
    }
}

/// Parse the Int stepper's buffer back into a `u16` clamped to the
/// `max_thoughts_width` registered bounds. Defensive — the stepper's
/// step path keeps the buffer in range, but a synthetic test fixture
/// or a future code path could seed an out-of-range buffer.
///
/// Both `MIN = 40` and `MAX = 500` fit inside `u16`, so the `clamp`
/// result is always non-negative and ≤ `u16::MAX`. We use
/// `u16::try_from(...).unwrap_or(u16::MAX)` instead of `as u16` so
/// a future bump to `MAX > u16::MAX` saturates rather than silently
/// truncating mod 65536 (security suggestion).
fn parse_max_thoughts_width_buffer(buffer: &str) -> u16 {
    let clamped = buffer
        .parse::<i64>()
        .unwrap_or(crate::settings::defs::MAX_THOUGHTS_WIDTH_MIN)
        .clamp(
            crate::settings::defs::MAX_THOUGHTS_WIDTH_MIN,
            crate::settings::defs::MAX_THOUGHTS_WIDTH_MAX,
        );
    u16::try_from(clamped).unwrap_or(u16::MAX)
}

/// Render the `max_thoughts_width` live wrap preview block.
///
/// **Vertical layout inside `area`.** Top-anchored. Row 0 of `area`
/// is a 1-row blank gap separating the preview from whatever sits
/// directly above (in the live caller, the stepper row); row 1 is
/// the title; rows 2..(2+content_rows) hold the wrapped content.
/// When `pending_value > area.width` (the preview is clamped) and
/// there are at least two rows of vertical slack below the
/// content, a 1-row blank gap and then a note row carry the text
/// `note: clamped at N cols`. Any rows below the note
/// stay blank — that empty space is intentional and stays
/// unpainted (the chrome footer sits below `inner_area`).
///
/// **Edge cases (per spec).**
/// - `area.width < MAX_THOUGHTS_WIDTH_PREVIEW_MIN_WIDTH` (30): omit
///   the preview entirely — too narrow for readable wrapped text.
/// - `area.height < MAX_THOUGHTS_WIDTH_PREVIEW_MIN_HEIGHT` (5):
///   omit — insufficient vertical budget for the gap + title +
///   2 content rows layout.
/// - `pending_value > area.width`: clamp the preview width to
///   `area.width`. The title stays plain `preview`; the clamp
///   amount is surfaced via a `note: clamped at N cols` row
///   rendered below the content when there's a row of slack
///   below it. Content takes priority over the note — if there's
///   no room for the note row, it's silently omitted.
/// - Active setting key gating happens at the call site
///   (`setting_key == "max_thoughts_width"`); this helper is pure
///   on the `pending_value` it receives.
///
/// **Theme tokens.**
/// - Title bg: `theme.bg_visual` — the heavier / more saturated of
///   the two "block" bg tokens; matches selection-bg saturation.
/// - Content bg: `theme.bg_highlight` — the lighter / less
///   saturated of the two; still distinguishable from
///   `theme.bg_base` so the preview reads as a contained block.
/// - Title fg + content fg: `theme.text_primary` — same color the
///   scrollback's thinking output renders in. Italic + bold +
///   underlined for the title (the UNDERLINED gives consistent
///   additional visual weight on themes where `bg_visual` vs
///   `bg_highlight` is mostly a hue shift rather than a luma shift,
///   e.g. TokyoNight); italic only for content.
fn render_max_thoughts_width_preview(
    buf: &mut Buffer,
    area: Rect,
    pending_value: u16,
    theme: &Theme,
) {
    // Edge case 1: terminal area too narrow → omit.
    if area.width < MAX_THOUGHTS_WIDTH_PREVIEW_MIN_WIDTH {
        return;
    }
    // Edge case 2: terminal area too short → omit.
    if area.height < MAX_THOUGHTS_WIDTH_PREVIEW_MIN_HEIGHT {
        return;
    }
    // Defensive guard: catch future editors who add `\n` / `\t` (or
    // any other control char that bypasses word_wrap_line's flow)
    // to the sample. `wrap_description` has the same debug_assert
    // for the same reason.
    debug_assert!(
        !MAX_THOUGHTS_WIDTH_PREVIEW_SAMPLE.contains('\n')
            && !MAX_THOUGHTS_WIDTH_PREVIEW_SAMPLE.contains('\t'),
        "MAX_THOUGHTS_WIDTH_PREVIEW_SAMPLE must not contain `\\n` or `\\t`; \
         word_wrap_line flattens spans byte-for-byte and would render control \
         cells as glyphs",
    );

    // Effective preview content width = min(pending, available).
    let pending_w = pending_value.max(1);
    let effective_width = pending_w.min(area.width);
    let clamped = pending_w > area.width;

    // Wrap the sample text at the effective width.
    let sample_line = Line::from(Span::raw(MAX_THOUGHTS_WIDTH_PREVIEW_SAMPLE));
    let wrapped = crate::render::wrapping::word_wrap_line(&sample_line, effective_width as usize);
    // Defensive: a degenerate wrap (zero lines) means we have no
    // meaningful preview to show. The MIN_WIDTH=30 gate above makes
    // this practically unreachable.
    if wrapped.is_empty() {
        return;
    }
    // Layout budget: 1 row blank gap (above title) + 1 row title +
    // N content rows. Cap N at the available rows; the rest of
    // `area` (below the last content row) stays blank. The
    // MIN_HEIGHT=5 gate above guarantees `area.height >= 5`, so
    // `available_content_rows >= 3` here — always enough room for
    // the minimum 2 content rows. We cap at `wrapped.len()` so a
    // short wrap doesn't leave us painting beyond the wrap shape.
    let available_content_rows = area.height.saturating_sub(2) as usize;
    let visible_content = wrapped.len().min(available_content_rows);
    render_preview_block(
        buf,
        area,
        effective_width,
        clamped,
        &wrapped[..visible_content],
        theme,
    );
}

/// Inner painter for the preview block — split out so the
/// caller's edge-case dispatch (omit / truncate / full-fit) stays
/// readable and the rendering logic isn't duplicated.
///
/// Caller guarantees:
/// - `effective_width <= area.width`.
/// - `wrapped.len() >= 1` AND `wrapped.len() + 2 <= area.height`
///   (1 row gap above + 1 row title + `wrapped.len()` content rows
///   must all fit inside `area`).
/// - `area.width >= MAX_THOUGHTS_WIDTH_PREVIEW_MIN_WIDTH`.
///
/// Clamped-state surfacing: when `clamped` is true and there are
/// at least 2 rows of vertical slack below the content (i.e.
/// `area.height >= wrapped.len() + 4`), a 1-row blank gap and
/// then a `note: clamped at N cols` row are rendered below the
/// last content row
/// in `theme.text_secondary` with no modifier and on
/// `theme.bg_base` (no block-tinted bg — the note lives in the
/// chrome strip below the preview, not inside the two-tone
/// preview block). When the slack is unavailable, the note is
/// silently omitted; content takes priority.
fn render_preview_block(
    buf: &mut Buffer,
    area: Rect,
    effective_width: u16,
    clamped: bool,
    wrapped: &[Line<'_>],
    theme: &Theme,
) {
    debug_assert!(
        area.height >= (wrapped.len() as u16).saturating_add(2),
        "render_preview_block caller-guarantee violated: \
         area.height={} < wrapped.len()+2={}",
        area.height,
        wrapped.len() + 2,
    );
    // Top-anchor: row 0 of `area` is a blank gap, row 1 holds the
    // title, rows 2..(2+content_rows) hold the wrapped content.
    // Any rows below the last content row stay blank, except for
    // the optional clamped-note row described at the bottom of
    // this function.
    let title_y = area.y.saturating_add(1);

    // ── Title row. ────────────────────────────────────────────────
    let title_bg = theme.bg_visual;
    let content_bg = theme.bg_highlight;
    let title_fg = theme.text_primary;
    let content_fg = theme.text_primary;

    // Paint title bg first so partial-trailing-whitespace stays
    // tinted (the bg extends to the FULL effective_width on every
    // row, including any title columns past the text).
    let title_rect = Rect {
        x: area.x,
        y: title_y,
        width: effective_width,
        height: 1,
    };
    buf.set_style(title_rect, Style::default().bg(title_bg));

    // Title is always plain lowercase `preview`. The previous
    // implementation appended ` · clamped to N cols` to the title
    // when the preview clamped to a narrower terminal width; the
    // clamp signal has been moved to a note row below the content
    // (see the bottom of this function) so the title carries the
    // same shape regardless of clamp state.
    let title_text: &str = "preview";
    let title_text_truncated: std::borrow::Cow<'_, str> =
        if title_text.width() <= effective_width as usize {
            std::borrow::Cow::Borrowed(title_text)
        } else {
            std::borrow::Cow::Owned(truncate_str(title_text, effective_width as usize))
        };
    let title_w = (title_text_truncated.width() as u16).min(effective_width);
    // BOLD + ITALIC + UNDERLINED on the title. UNDERLINED gives
    // additional visual weight independent of the bg luma — on
    // TokyoNight `bg_visual` vs `bg_highlight` is
    // mostly a hue shift, not a luma shift, so the underline
    // carries the "this is the title" cue on its own.
    let title_style = Style::default()
        .fg(title_fg)
        .bg(title_bg)
        .add_modifier(Modifier::BOLD | Modifier::ITALIC | Modifier::UNDERLINED);
    buf.set_span(
        area.x,
        title_y,
        &Span::styled(title_text_truncated.as_ref(), title_style),
        title_w,
    );

    // ── Content rows. ─────────────────────────────────────────────
    let content_style = Style::default()
        .fg(content_fg)
        .bg(content_bg)
        .add_modifier(Modifier::ITALIC);
    for (i, wrap_line) in wrapped.iter().enumerate() {
        let row_y = title_y + 1 + i as u16;
        // Paint bg first across `effective_width` so trailing
        // whitespace on a wrap line is still tinted.
        let row_rect = Rect {
            x: area.x,
            y: row_y,
            width: effective_width,
            height: 1,
        };
        buf.set_style(row_rect, Style::default().bg(content_bg));
        // Flatten the wrapped line's spans back to a plain string
        // (the sample text has no inline styles, so we don't lose
        // any styling). Then re-style with our italic + content_fg.
        let text: String = wrap_line
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>();
        let text_w = (text.width() as u16).min(effective_width);
        if text_w > 0 {
            buf.set_span(area.x, row_y, &Span::styled(text, content_style), text_w);
        }
    }

    // ── Clamped note (optional, height-permitting). ───────────────
    //
    // When `clamped`, surface the clamp in a low-key note row
    // immediately below the last content row. The note is
    // height-aware: content takes priority, so if there's no row
    // of slack below the content we omit the note entirely. The
    // note sits OUTSIDE the two-tone preview bg block — it uses
    // `theme.bg_base` so it visually reads as chrome/tip text,
    // not as part of the wrap preview. Left-aligned at the same
    // x-offset as content (`area.x`).
    if clamped {
        // One blank row sits between the last content row and the
        // note so the note reads as a separate annotation, not a
        // continuation of the wrap preview.
        let note_y = title_y
            .saturating_add(1)
            .saturating_add(wrapped.len() as u16)
            .saturating_add(1);
        let area_end_y = area.y.saturating_add(area.height);
        if note_y < area_end_y {
            let note_text = format!("note: clamped at {effective_width} cols");
            let note_text_truncated: std::borrow::Cow<'_, str> =
                if note_text.width() <= area.width as usize {
                    std::borrow::Cow::Borrowed(note_text.as_str())
                } else {
                    std::borrow::Cow::Owned(truncate_str(&note_text, area.width as usize))
                };
            let note_w = (note_text_truncated.width() as u16).min(area.width);
            // No modifier, no bg tint — the note reads as chrome
            // text aligned with the preview's left edge.
            let note_style = Style::default().fg(theme.text_secondary).bg(theme.bg_base);
            buf.set_span(
                area.x,
                note_y,
                &Span::styled(note_text_truncated.as_ref(), note_style),
                note_w,
            );
        }
    }
}

/// `compute_max_label_w` equivalent for settings rows. Caps the column
/// at 24 cols (so a single outlier label can't push the value column
/// off-screen) and never exceeds half the content area width.
/// Mirrors `question_view::compute_max_label_w` semantics.
fn compute_settings_max_label_w(metas: &[SettingMeta], content_w: u16) -> u16 {
    const MAX_LABEL_W: u16 = 24;
    let half = content_w / 2;
    let cap = MAX_LABEL_W.min(half);
    metas
        .iter()
        .map(|m| m.label.width() as u16)
        .max()
        .unwrap_or(0)
        .min(cap)
}

/// Look up the user-friendly display string for an Enum canonical
/// against the setting's own `EnumChoice` catalog. Falls back to the
/// canonical verbatim if the lookup misses (defense-in-depth: a
/// hand-edited corrupted config with an unknown canonical still
/// renders without an empty string, mirroring
/// `display_name_for_canonical`'s pattern).
///
/// Look up the display name for an Enum canonical via the registry.
fn display_for_enum_canonical<'a>(kind: &'a SettingKind, canonical: &'a str) -> &'a str {
    if let SettingKind::Enum { choices, .. } = kind {
        for c in *choices {
            if c.canonical == canonical {
                return c.display;
            }
        }
    }
    // Fallback: render the canonical verbatim. Defensive — catches a
    // schema-vs-renderer drift without crashing the modal.
    canonical
}

/// Word-wrap a description string. Returns owned lines for re-styling.
/// Asserts descriptions are single-line (no `\n`/`\t`).
fn wrap_description(description: &str, width: u16) -> Vec<String> {
    if description.is_empty() || width == 0 {
        return Vec::new();
    }
    debug_assert!(
        !description.contains('\n') && !description.contains('\t'),
        "SettingMeta::description is single-line / no tabs by contract: \
         description={description:?}. Word-wrap doesn't split on \\n or \\t — \
         such chars would render as control codes in a buffer cell. \
         Pre-split + iterate if multi-line descriptions become useful.",
    );
    let line = Line::from(Span::raw(description));
    crate::render::wrapping::word_wrap_line(&line, width as usize)
        .into_iter()
        .map(|l| {
            l.spans
                .into_iter()
                .map(|s| s.content.into_owned())
                .collect::<String>()
        })
        .collect()
}

// Row layout: triangle on left, value right-aligned. Two-line
// layout used when label + value exceed area width.

// Row chrome dimensions.
const ROW_TRIANGLE_PREFIX_W: u16 = 2;
const ROW_GAP_MIN_W: u16 = 1;
const ROW_RIGHT_PAD_W: u16 = 1;
const ROW_CHEVRON_W: u16 = 2;
/// Chevron column width — reserved for all rows for alignment.
const ROW_CHEVRON_COL_W: u16 = ROW_CHEVRON_W;
const ROW_RESTART_PILL_W: u16 = 10; // " · restart" — used for layout budgeting only.

/// Per-row layout decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowLayout {
    OneLine,
    /// Value drops to line 2 (label too wide for single line).
    TwoLine,
    /// Even label alone exceeds width — truncate label, value on line 2.
    TwoLineWithLabelTruncation,
}

/// Decide whether a setting row needs 1 or 2 logical lines.
fn row_layout(
    area_width: u16,
    label: &str,
    value_display: &str,
    show_restart_pill: bool,
) -> RowLayout {
    let restart_w = if show_restart_pill {
        ROW_RESTART_PILL_W
    } else {
        0
    };
    let label_w = label.width() as u16;
    let value_w = value_display.width() as u16;
    let one_line_total = ROW_TRIANGLE_PREFIX_W
        .saturating_add(label_w)
        .saturating_add(ROW_GAP_MIN_W)
        .saturating_add(value_w)
        .saturating_add(ROW_CHEVRON_COL_W)
        .saturating_add(restart_w)
        .saturating_add(ROW_RIGHT_PAD_W);
    if one_line_total <= area_width {
        return RowLayout::OneLine;
    }
    // Two-line: line 1 hosts the label + (optional) restart pill +
    // right pad. If even that doesn't fit, fall back to label
    // truncation on line 1.
    let line1_full = ROW_TRIANGLE_PREFIX_W
        .saturating_add(label_w)
        .saturating_add(restart_w)
        .saturating_add(ROW_RIGHT_PAD_W);
    if line1_full <= area_width {
        RowLayout::TwoLine
    } else {
        RowLayout::TwoLineWithLabelTruncation
    }
}
#[allow(clippy::too_many_arguments)]
fn render_setting_row(
    buf: &mut Buffer,
    area: Rect,
    meta: &SettingMeta,
    value: &SettingValue,
    max_label_w: u16,
    is_selected: bool,
    theme: &Theme,
    is_expanded: bool,
    is_hovered: bool,
) -> Rect {
    let bg = if is_selected {
        theme.bg_visual
    } else if is_hovered {
        theme.bg_hover
    } else {
        theme.bg_base
    };
    // Paint the row bg across the full area (1 or 2 lines).
    buf.set_style(area, Style::default().bg(bg));

    let label_style = Style::default().fg(theme.text_primary).bg(bg);
    // Bool(false) renders muted; all other values use accent.
    let value_style = Style::default().fg(theme.accent_user).bg(bg);
    let chevron_style = Style::default().fg(theme.gray).bg(bg);
    let restart_style = Style::default()
        .fg(theme.gray_dim)
        .bg(bg)
        .add_modifier(Modifier::ITALIC);
    let desc_style = Style::default().fg(theme.gray).bg(bg);

    // Enum rows display the user-friendly name, not the canonical.
    let value_text_owned;
    let value_text: &str = match value {
        SettingValue::Bool(b) => {
            if *b {
                "on"
            } else {
                "off"
            }
        }
        SettingValue::String(s) => {
            if s.is_empty() && matches!(meta.kind, SettingKind::DynamicEnum { .. }) {
                "(no override)"
            } else {
                s.as_str()
            }
        }
        SettingValue::Enum(e) => display_for_enum_canonical(&meta.kind, e),
        SettingValue::Int(i) => {
            value_text_owned = i.to_string();
            &value_text_owned
        }
    };

    let value_style = if matches!(value, SettingValue::Bool(false)) {
        Style::default().fg(theme.gray).bg(bg)
    } else {
        value_style
    };

    // Chevron for Enum/String/DynamicEnum (opens picker/editor).
    let show_chevron = matches!(
        (&meta.kind, value),
        (SettingKind::Enum { .. }, _)
            | (SettingKind::String { .. }, _)
            | (SettingKind::DynamicEnum { .. }, _)
    );
    let chevron_str = format!(" {}", crate::glyphs::chevron()); // › → > on legacy ConHost
    let chevron_w = if show_chevron {
        chevron_str.width() as u16
    } else {
        0
    };
    let value_w = value_text.width() as u16;

    // Pill only while expanded — change-time feedback is the toast's job, and
    // a collapsed non-default row would misread as "restart pending" forever.
    let show_restart_pill = meta.restart_required && is_expanded;
    let restart_pill_text = " \u{00B7} restart";
    let restart_w = if show_restart_pill {
        restart_pill_text.width() as u16
    } else {
        0
    };

    // Triangle prefix: "▸" collapsed, "▾" expanded.
    let triangle = if is_expanded { "\u{25BE}" } else { "\u{25B8}" };
    debug_assert_eq!(
        triangle.width(),
        (ROW_TRIANGLE_PREFIX_W - 1) as usize,
        "ROW_TRIANGLE_PREFIX_W = {ROW_TRIANGLE_PREFIX_W} assumes a 1-cell triangle; \
         glyph `{triangle}` measures {} cells. A 2-cell triangle (e.g. ▶ / ▼ from \
         fold_indicator_span) would shift the entire row column. Update the constant \
         or pick a 1-cell glyph.",
        triangle.width(),
    );

    // Fall back to one-line if only 1 line was allocated.
    let layout_decision = row_layout(area.width, meta.label, value_text, show_restart_pill);
    let layout = if area.height < 2 {
        // Only 1 line available — collapse to a one-line render and
        // accept that the label might collide with the value column.
        RowLayout::OneLine
    } else {
        layout_decision
    };
    let _ = max_label_w;

    // ── Compute right-side x positions (shared across layouts). ──
    // Layout (right-to-left): [restart pill][space][chevron][space][value]
    // The 1-cell right pad is baked into `restart_x`.
    let restart_x_line1 = (area.x + area.width).saturating_sub(restart_w + 1);

    match layout {
        RowLayout::OneLine => {
            // Chevron column reserved for all rows for alignment.
            let chevron_x = restart_x_line1.saturating_sub(ROW_CHEVRON_COL_W);
            let value_x = chevron_x.saturating_sub(value_w + 1);

            let label_text = format!("{triangle} {}", meta.label);
            let label_w = label_text.width() as u16;
            let label_max_x = area.x.saturating_add(label_w);
            // Cap label end at value_x to never collide with the value column.
            let label_end = label_max_x.min(value_x.saturating_sub(1));
            let label_used = label_end.saturating_sub(area.x);

            if label_used > 0 {
                buf.set_span(
                    area.x,
                    area.y,
                    &Span::styled(&label_text, label_style),
                    label_used,
                );
            }

            if value_x > area.x.saturating_add(label_used) {
                buf.set_span(
                    value_x,
                    area.y,
                    &Span::styled(value_text, value_style),
                    value_w,
                );
            }
            if show_chevron && chevron_w > 0 && chevron_x >= area.x.saturating_add(label_used) {
                buf.set_span(
                    chevron_x,
                    area.y,
                    &Span::styled(chevron_str.as_str(), chevron_style),
                    chevron_w,
                );
            }
            if show_restart_pill && restart_w > 0 {
                buf.set_span(
                    restart_x_line1,
                    area.y,
                    &Span::styled(restart_pill_text, restart_style),
                    restart_w,
                );
            }

            let _ = desc_style;
            let _ = is_selected;

            // Hit-rect for the value column: spans the value text
            // plus the (always-reserved) chevron column. Clicking
            // the chevron column on a Bool row is a no-op (no
            // glyph there) but still routes to the row, matching
            // chevron rows.
            Rect {
                x: value_x,
                y: area.y,
                width: value_w.saturating_add(ROW_CHEVRON_COL_W),
                height: 1,
            }
        }
        RowLayout::TwoLine | RowLayout::TwoLineWithLabelTruncation => {
            // ── Line 1: triangle + label + (restart pill) ──
            // Compute how much horizontal space is available to the
            // label before colliding with the restart pill.
            let label_avail = area
                .width
                .saturating_sub(restart_w + 1) // restart pill + right pad
                .saturating_sub(ROW_TRIANGLE_PREFIX_W);

            let label_text_owned: String;
            let label_text: &str = match layout {
                RowLayout::TwoLineWithLabelTruncation => {
                    // Truncate the label so triangle + truncated label
                    // + restart_pill + right_pad fits on line 1.
                    if label_avail == 0 {
                        ""
                    } else {
                        label_text_owned = truncate_str(meta.label, label_avail as usize);
                        &label_text_owned
                    }
                }
                _ => meta.label,
            };

            let full_label_text = format!("{triangle} {label_text}");
            let full_label_w = full_label_text.width() as u16;
            let label_used = full_label_w.min(area.width.saturating_sub(restart_w + 1));

            if label_used > 0 {
                buf.set_span(
                    area.x,
                    area.y,
                    &Span::styled(&full_label_text, label_style),
                    label_used,
                );
            }
            if show_restart_pill && restart_w > 0 {
                buf.set_span(
                    restart_x_line1,
                    area.y,
                    &Span::styled(restart_pill_text, restart_style),
                    restart_w,
                );
            }

            // ── Line 2: right-aligned value + chevron column ──
            //
            // The chevron column is reserved
            // for ALL rows so the `›` glyph is at a constant
            // offset; Bool rows leave it empty but the value
            // still right-aligns to the column's left edge.
            // An earlier version anchored Bool rows on line 2 to
            // `area.right - value_w - 1` (no chevron column
            // reserved), shifting their `on`/`off` text 2 cells
            // to the right of chevron rows' values — a
            // visual misalignment.
            //
            // Anchor line-2's
            // chevron-column LEFT EDGE at the same column the
            // one-line layout uses: `area.right - ROW_RIGHT_PAD_W
            // - ROW_CHEVRON_COL_W` (i.e. `restart_x_line1 -
            // ROW_CHEVRON_COL_W` when no restart pill is on
            // line 2). The earlier version anchored at
            // `area.right - ROW_CHEVRON_COL_W`, so on a row
            // that flipped from one-line to two-line layout the
            // `›` glyph would jump 1 cell rightward — producing
            // a staircase between mixed-layout rows. Subtracting
            // `ROW_RIGHT_PAD_W` here brings line 2 into pixel
            // parity with line 1.
            let y2 = area.y + 1;
            let chevron_x_line2 = (area.x + area.width)
                .saturating_sub(ROW_RIGHT_PAD_W + ROW_CHEVRON_COL_W)
                .max(area.x);
            let value_x_line2 = chevron_x_line2.saturating_sub(value_w + 1).max(area.x);

            // Render value, then chevron, on line 2. Clip if either
            // would land off the left edge in a pathologically
            // narrow row.
            if value_w > 0 && value_x_line2 + value_w <= area.x + area.width {
                buf.set_span(
                    value_x_line2,
                    y2,
                    &Span::styled(value_text, value_style),
                    value_w,
                );
            }
            if show_chevron
                && chevron_w > 0
                && chevron_x_line2 + ROW_CHEVRON_COL_W <= area.x + area.width
            {
                buf.set_span(
                    chevron_x_line2,
                    y2,
                    &Span::styled(chevron_str.as_str(), chevron_style),
                    chevron_w,
                );
            }

            let _ = desc_style;
            let _ = is_selected;

            // Hit-rect for the value column: covers the value text
            // + the always-reserved chevron column on LINE 2 only.
            // Width is `value_w + ROW_CHEVRON_COL_W`
            // (not `value_w + chevron_w`) so the hit-rect spans the
            // empty chevron column on Bool rows too.
            Rect {
                x: value_x_line2,
                y: y2,
                width: value_w.saturating_add(ROW_CHEVRON_COL_W),
                height: 1,
            }
        }
    }
}

/// Render the wrapped description for an expanded row.
fn render_expanded_description(buf: &mut Buffer, area: Rect, meta: &SettingMeta, theme: &Theme) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let desc_style = Style::default()
        .fg(theme.gray)
        .bg(theme.bg_base)
        .add_modifier(Modifier::ITALIC);
    let desc_src: &str = meta.description;
    // Indent 4 cols to nest under the label.
    let indent = 4u16.min(area.width);
    let wrap_w = area.width.saturating_sub(indent);
    if wrap_w == 0 {
        return;
    }
    let line = Line::from(Span::styled(desc_src, desc_style));
    let wrapped = crate::render::wrapping::word_wrap_line(&line, wrap_w as usize);
    for (i, wrapped_line) in wrapped.iter().enumerate() {
        if (i as u16) >= area.height {
            break;
        }
        let y = area.y + i as u16;
        // Paint indent bg first so the wrapped text aligns visually.
        for x in area.x..area.x + indent {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_bg(theme.bg_base);
            }
        }
        buf.set_line(area.x + indent, y, wrapped_line, wrap_w);
    }
}

/// Fallback render path for a row whose `current_value_for` returned
/// `None` (registry / dispatch skew). Shows the label without a value
/// column so the misconfiguration is visible at runtime; the
/// `every_setting_has_dispatch_arm` test catches the case at CI time.
fn render_setting_row_no_value(
    buf: &mut Buffer,
    area: Rect,
    meta: &SettingMeta,
    max_label_w: u16,
    is_selected: bool,
    theme: &Theme,
) {
    let bg = if is_selected {
        theme.bg_visual
    } else {
        theme.bg_base
    };
    buf.set_style(area, Style::default().bg(bg));
    let label_style = Style::default()
        .fg(theme.accent_error)
        .bg(bg)
        .add_modifier(Modifier::BOLD);

    let label_max_w = max_label_w;
    let label_truncated: std::borrow::Cow<'_, str> = if meta.label.width() <= label_max_w as usize {
        std::borrow::Cow::Borrowed(meta.label)
    } else {
        std::borrow::Cow::Owned(truncate_str(meta.label, label_max_w as usize))
    };
    let text = format!(" !   {label_truncated} (no read mapping)");
    let w = text.width() as u16;
    buf.set_span(
        area.x,
        area.y,
        &Span::styled(&text, label_style),
        w.min(area.width),
    );
}

/// Render a `Group` row in the Browse list: a triangle-prefixed label with a
/// trailing chevron (opens the sub-sheet). Carries no value column. Returns the
/// chevron hit-rect so a click on it opens the sub-sheet like an Enum row.
fn render_setting_group_row(
    buf: &mut Buffer,
    area: Rect,
    meta: &SettingMeta,
    is_selected: bool,
    is_hovered: bool,
    is_expanded: bool,
    theme: &Theme,
) -> Rect {
    let bg = if is_selected {
        theme.bg_visual
    } else if is_hovered {
        theme.bg_hover
    } else {
        theme.bg_base
    };
    buf.set_style(area, Style::default().bg(bg));
    let label_style = Style::default().fg(theme.text_primary).bg(bg);
    let chevron_style = Style::default().fg(theme.gray).bg(bg);

    let chevron_str = format!(" {}", crate::glyphs::chevron());
    let chevron_w = chevron_str.width() as u16;
    let chevron_x = (area.x + area.width)
        .saturating_sub(ROW_RIGHT_PAD_W)
        .saturating_sub(ROW_CHEVRON_COL_W);

    // Triangle prefix mirrors normal rows: "▾" expanded, "▸" collapsed
    // (the group's description expands inline via Right/l like other rows).
    let triangle = if is_expanded { "\u{25BE}" } else { "\u{25B8}" };
    let label_text = format!("{triangle} {}", meta.label);
    let label_cap = chevron_x.saturating_sub(area.x).saturating_sub(1);
    let label_w = (label_text.width() as u16).min(label_cap);
    if label_w > 0 {
        buf.set_span(
            area.x,
            area.y,
            &Span::styled(&label_text, label_style),
            label_w,
        );
    }
    if chevron_w > 0 && chevron_x >= area.x.saturating_add(label_w) {
        buf.set_span(
            chevron_x,
            area.y,
            &Span::styled(chevron_str.as_str(), chevron_style),
            chevron_w,
        );
    }
    // Hit-rect spans the chevron column (a click there opens the sub-sheet).
    Rect {
        x: chevron_x,
        y: area.y,
        width: ROW_CHEVRON_COL_W,
        height: 1,
    }
}

/// Build the footer shortcut row. Enter label varies by focused row kind.
fn build_shortcuts(state: &SettingsModalState) -> Vec<Shortcut<'static>> {
    match state.mode {
        SettingsModalMode::Browse => {
            let enter_label = match state.focused_setting() {
                Some((_, meta)) if matches!(meta.kind, SettingKind::Bool { .. }) => "Enter toggle",
                _ => "Enter edit",
            };
            let mut shortcuts = vec![
                Shortcut {
                    label: "\u{2191}/\u{2193}/j/k nav",
                    clickable: false,
                    id: 0,
                },
                Shortcut {
                    label: "g/G top/btm",
                    clickable: false,
                    id: 0,
                },
                Shortcut {
                    label: "Space toggle",
                    clickable: false,
                    id: 0,
                },
                Shortcut {
                    label: enter_label,
                    clickable: false,
                    id: 0,
                },
                Shortcut {
                    label: "\u{2192} expand",
                    clickable: false,
                    id: 0,
                },
                Shortcut {
                    label: "/ search",
                    clickable: false,
                    id: 0,
                },
                Shortcut {
                    label: "d reset",
                    clickable: false,
                    id: 0,
                },
                Shortcut {
                    label: "F2/Esc close",
                    clickable: false,
                    id: 0,
                },
            ];
            // Browse is nav mode (filter inactive), so append `i search` last
            // (matching the shared pickers).
            modal_window::push_vim_nav_search_hint(&mut shortcuts, false);
            shortcuts
        }
        SettingsModalMode::FilterFocused => vec![
            Shortcut {
                label: "type to filter",
                clickable: false,
                id: 0,
            },
            Shortcut {
                label: "\u{2191}/\u{2193} nav",
                clickable: false,
                id: 0,
            },
            Shortcut {
                label: "Backspace edit",
                clickable: false,
                id: 0,
            },
            Shortcut {
                label: "Enter commit",
                clickable: false,
                id: 0,
            },
            Shortcut {
                label: "Esc clear",
                clickable: false,
                id: 0,
            },
        ],
        SettingsModalMode::PickingEnum {
            supports_preview: sp,
            ..
        } => {
            // Labels depend on whether the Enum supports live preview.
            let nav_label = if sp {
                "\u{2191}/\u{2193} try"
            } else {
                "\u{2191}/\u{2193} nav"
            };
            let esc_label = if sp { "Esc revert" } else { "Esc cancel" };
            vec![
                Shortcut {
                    label: nav_label,
                    clickable: false,
                    id: 0,
                },
                Shortcut {
                    label: "Enter commit",
                    clickable: false,
                    id: 0,
                },
                Shortcut {
                    label: esc_label,
                    clickable: false,
                    id: 0,
                },
                Shortcut {
                    label: "d reset",
                    clickable: false,
                    id: 0,
                },
            ]
        }

        SettingsModalMode::EditingValue { key, .. } => {
            // Int stepper: step-only hints with range-aware deltas.
            if let Some(SettingKind::Int { min, max, .. }) =
                state.registry.find(key).map(|m| &m.kind)
            {
                let (small_label, large_label) = int_step_footer_labels(*min, *max);
                return vec![
                    Shortcut {
                        label: small_label,
                        clickable: false,
                        id: 0,
                    },
                    Shortcut {
                        label: large_label,
                        clickable: false,
                        id: 0,
                    },
                    Shortcut {
                        label: "Enter commit",
                        clickable: false,
                        id: 0,
                    },
                    Shortcut {
                        label: "Esc cancel",
                        clickable: false,
                        id: 0,
                    },
                    Shortcut {
                        label: "d reset",
                        clickable: false,
                        id: 0,
                    },
                ];
            }
            vec![
                Shortcut {
                    label: "type to edit",
                    clickable: false,
                    id: 0,
                },
                Shortcut {
                    label: "\u{2190}/\u{2192} cursor",
                    clickable: false,
                    id: 0,
                },
                Shortcut {
                    label: "Enter commit",
                    clickable: false,
                    id: 0,
                },
                Shortcut {
                    label: "Esc cancel",
                    clickable: false,
                    id: 0,
                },
            ]
        }
        SettingsModalMode::PickingGroup { .. } => vec![
            Shortcut {
                label: "\u{2191}/\u{2193}/j/k nav",
                clickable: false,
                id: 0,
            },
            Shortcut {
                label: "Space/Enter toggle",
                clickable: false,
                id: 0,
            },
            Shortcut {
                label: "Esc back",
                clickable: false,
                id: 0,
            },
        ],
    }
}

// ---------------------------------------------------------------------------
// Key handling
// ---------------------------------------------------------------------------

/// Handle a key event in the settings modal.
///
/// F2/Ctrl+,/Cmd+, always close regardless of mode. Esc behavior is
/// mode-dependent: Browse delegates to chrome, sub-modes handle it
/// locally (FilterFocused clears query, PickingEnum reverts preview,
/// EditingValue cancels). Space/Enter Repeat events are suppressed
/// to avoid per-tick disk writes.
pub fn handle_settings_key(state: &mut SettingsModalState, key: &KeyEvent) -> SettingsKeyOutcome {
    if key.kind == KeyEventKind::Release {
        return SettingsKeyOutcome::Unchanged;
    }

    // Suppress Repeat for toggle keys to avoid per-tick disk writes.
    if key.kind == KeyEventKind::Repeat && matches!(key.code, KeyCode::Char(' ') | KeyCode::Enter) {
        return SettingsKeyOutcome::Unchanged;
    }

    if is_close_key(key) {
        return SettingsKeyOutcome::Close;
    }

    // Exhaustive per-mode dispatch.
    match state.mode {
        SettingsModalMode::Browse => handle_browse(state, key),
        SettingsModalMode::FilterFocused => handle_filter_focused(state, key),
        SettingsModalMode::PickingEnum { .. } => handle_picking_enum(state, key),
        SettingsModalMode::PickingGroup { .. } => handle_picking_group(state, key),
        SettingsModalMode::EditingValue { .. } => handle_editing_value(state, key),
    }
}

/// Enum chooser key routing. Up/Down dispatches preview actions,
/// Enter commits current choice, Esc reverts to original value.
fn handle_picking_enum(state: &mut SettingsModalState, key: &KeyEvent) -> SettingsKeyOutcome {
    // Snapshot the current picker state under an immutable borrow so
    // the subsequent `state.mode = ...` writes are unambiguous.
    let (setting_key, choices_idx, original_value, supports_preview) = match &state.mode {
        SettingsModalMode::PickingEnum {
            key,
            choices_idx,
            original_value,
            supports_preview,
        } => (
            *key,
            *choices_idx,
            original_value.clone(),
            *supports_preview,
        ),
        _ => return SettingsKeyOutcome::Unchanged,
    };

    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            let len = picker_choices_len(state, setting_key);
            if choices_idx + 1 >= len {
                return SettingsKeyOutcome::Unchanged;
            }
            set_picker_idx(
                state,
                setting_key,
                choices_idx + 1,
                original_value,
                supports_preview,
            )
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if choices_idx == 0 {
                return SettingsKeyOutcome::Unchanged;
            }
            set_picker_idx(
                state,
                setting_key,
                choices_idx - 1,
                original_value,
                supports_preview,
            )
        }
        KeyCode::Enter => {
            // Commit: dispatch the typed COMMIT Action for the
            // currently-focused choice. This is the single place
            // per picker open → close cycle that fires
            // `Effect::PersistSetting`, eliminating the per-keystroke
            // disk write race. The most recent PREVIEW Action (from Up/Down) has
            // already mutated the live visual; the commit's
            // `set_X_inner` is idempotent on that.
            //
            // For
            // `SettingKind::DynamicEnum` settings (e.g.
            // `default_model`, `fork_secondary_model`), the picker
            // commits through `action_for_string` rather than
            // `action_for_enum_commit` — the canonical is a runtime
            // string sourced from the model catalog, which
            // `action_for_string` already knows how to resolve via
            // `snapshot.resolve_model_name` AND treats the empty
            // canonical as a `Clear*` sentinel.
            let kind_is_dynamic = matches!(
                state.registry.find(setting_key).map(|m| &m.kind),
                Some(SettingKind::DynamicEnum { .. })
            );
            state.transition_to_browse();
            if kind_is_dynamic {
                let Some(canonical) = picker_choice_at_owned(state, setting_key, choices_idx)
                else {
                    return SettingsKeyOutcome::Changed;
                };
                if let Some(action) =
                    action_for_string(setting_key, canonical, &state.pager_snapshot)
                {
                    return SettingsKeyOutcome::Action(action);
                }
                return SettingsKeyOutcome::Changed;
            }
            let Some(current_canonical) = picker_choice_at(state, setting_key, choices_idx) else {
                return SettingsKeyOutcome::Changed;
            };
            if let Some(action) = action_for_enum_commit(setting_key, current_canonical) {
                return SettingsKeyOutcome::Action(action);
            }
            SettingsKeyOutcome::Changed
        }
        KeyCode::Esc => {
            // Revert preview and return to Browse. Non-preview Enums
            // skip the revert (no live visual was applied).
            state.transition_to_browse();
            if let SettingValue::Enum(orig) = &original_value
                && let Some(action) = action_for_enum(setting_key, orig)
            {
                return SettingsKeyOutcome::Action(action);
            }
            SettingsKeyOutcome::Changed
        }
        // `d` reset: close picker, revert preview if applicable,
        // then open the reset-confirm overlay.
        KeyCode::Char('d') if key.modifiers.is_empty() => {
            state.transition_to_browse();
            if supports_preview
                && let SettingValue::Enum(orig) = &original_value
                && let Some(revert) = action_for_enum(setting_key, orig)
            {
                return SettingsKeyOutcome::ActionPair(
                    revert,
                    Action::OpenResetConfirm { key: setting_key },
                );
            }
            SettingsKeyOutcome::Action(Action::OpenResetConfirm { key: setting_key })
        }
        _ => SettingsKeyOutcome::Unchanged,
    }
}

/// The children of a group setting, or an empty slice if `key` is not a group.
fn group_children(state: &SettingsModalState, key: SettingKey) -> &'static [SettingKey] {
    match state.registry.find(key).map(|m| &m.kind) {
        Some(SettingKind::Group { children }) => children,
        _ => &[],
    }
}

/// Group sub-sheet key routing. Up/Down moves between the child toggles;
/// Space/Enter toggles the focused child in place (the sheet stays open);
/// Esc returns to Browse.
fn handle_picking_group(state: &mut SettingsModalState, key: &KeyEvent) -> SettingsKeyOutcome {
    let (group_key, child_idx) = match &state.mode {
        SettingsModalMode::PickingGroup { key, child_idx } => (*key, *child_idx),
        _ => return SettingsKeyOutcome::Unchanged,
    };
    let children = group_children(state, group_key);
    if children.is_empty() {
        // Defensive: a group with no children can't be navigated — back out.
        state.transition_to_browse();
        return SettingsKeyOutcome::Changed;
    }

    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            if child_idx + 1 >= children.len() {
                return SettingsKeyOutcome::Unchanged;
            }
            state.mode = SettingsModalMode::PickingGroup {
                key: group_key,
                child_idx: child_idx + 1,
            };
            SettingsKeyOutcome::Changed
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if child_idx == 0 {
                return SettingsKeyOutcome::Unchanged;
            }
            state.mode = SettingsModalMode::PickingGroup {
                key: group_key,
                child_idx: child_idx - 1,
            };
            SettingsKeyOutcome::Changed
        }
        // Space/Enter toggle the focused child Bool and stay in the sheet so the
        // user can flip several tips in a row. The dispatcher refreshes the
        // modal snapshot, so the new value paints on the next frame.
        KeyCode::Char(' ') | KeyCode::Enter => {
            let Some(child_key) = children.get(child_idx).copied() else {
                return SettingsKeyOutcome::Unchanged;
            };
            let cur = match state.value_for(child_key) {
                Some(SettingValue::Bool(b)) => b,
                _ => return SettingsKeyOutcome::Unchanged,
            };
            match action_for_bool(child_key, !cur) {
                Some(action) => SettingsKeyOutcome::Action(action),
                None => SettingsKeyOutcome::Unchanged,
            }
        }
        KeyCode::Esc => {
            state.transition_to_browse();
            SettingsKeyOutcome::Changed
        }
        _ => SettingsKeyOutcome::Unchanged,
    }
}

/// Common nav body for Up/Down (and j/k aliases) in the picker:
/// update `choices_idx` in-place, look up the new canonical, fire
/// the preview dispatch via `action_for_enum`. Extracted from the
/// Update picker index and optionally dispatch a preview action.
/// `new_idx` must be in-bounds. Preview only fires for Enums with
/// `supports_preview: true`; side-effecting Enums skip preview.
fn set_picker_idx(
    state: &mut SettingsModalState,
    setting_key: SettingKey,
    new_idx: usize,
    original_value: SettingValue,
    supports_preview: bool,
) -> SettingsKeyOutcome {
    let in_bounds = new_idx < picker_choices_len(state, setting_key);
    if !in_bounds {
        // Caller bounds-checks before calling; belt-and-suspenders
        // for refactor safety.
        return SettingsKeyOutcome::Unchanged;
    }
    state.mode = SettingsModalMode::PickingEnum {
        key: setting_key,
        choices_idx: new_idx,
        supports_preview,
        original_value,
    };
    // Preview dispatch for static Enums with preview support.
    if supports_preview
        && let Some(new_canonical) = picker_choice_at(state, setting_key, new_idx)
        && let Some(action) = action_for_enum(setting_key, new_canonical)
    {
        return SettingsKeyOutcome::Action(action);
    }
    SettingsKeyOutcome::Changed
}

/// Inline string/int editor key routing. Esc cancels, Enter commits.
/// String mode: free-form text with cursor. Int mode: range-aware stepper
/// (Up/Down small, Left/Right large; see [`int_step_sizes`]), clamped to [min,max].
fn handle_editing_value(state: &mut SettingsModalState, key: &KeyEvent) -> SettingsKeyOutcome {
    // Snapshot mode payload under an immutable borrow.
    let (setting_key, buffer, cursor_byte, validation_error) = match &state.mode {
        SettingsModalMode::EditingValue {
            key,
            buffer,
            cursor_byte,
            validation_error,
        } => (*key, buffer.clone(), *cursor_byte, validation_error.clone()),
        _ => return SettingsKeyOutcome::Unchanged,
    };

    // Look up the registered kind so we know how to handle this
    // edit. The lookup is `&self`-only.
    let Some(meta) = state.registry.find(setting_key) else {
        // Registry skew — log and exit. The CI guards catch this.
        tracing::error!(
            target: "settings",
            key = setting_key,
            "EditingValue mode references an unregistered key — exiting to Browse",
        );
        state.transition_to_browse();
        return SettingsKeyOutcome::Changed;
    };
    let kind_snapshot = meta.kind.clone();

    // Int settings dispatch through a stepper-only
    // handler. All char-input / cursor-pan / Backspace / Delete /
    // Home / End keys are rejected; only Up/Down/Left/Right (and
    // j/k/h/l aliases), Enter, and Esc do anything.
    if matches!(kind_snapshot, SettingKind::Int { .. }) {
        return handle_int_stepper(state, key, setting_key, &buffer, &kind_snapshot);
    }

    match key.code {
        KeyCode::Esc => {
            state.transition_to_browse();
            SettingsKeyOutcome::Changed
        }
        KeyCode::Enter => {
            // Commit gate: re-validate against the current buffer.
            // On failure, refresh the inline error and stay in
            // EditingValue.
            let error = match &kind_snapshot {
                SettingKind::String { validator, .. } => {
                    validate_string(*validator, &buffer, &state.pager_snapshot.available_models)
                }
                _ => return SettingsKeyOutcome::Unchanged,
            };
            if error.is_some() {
                update_editing_value_buffer(state, buffer, cursor_byte, error);
                return SettingsKeyOutcome::Unchanged;
            }
            // Dispatch the typed Action and transition to Browse.
            let action_opt = match &kind_snapshot {
                SettingKind::String { .. } => {
                    action_for_string(setting_key, buffer.clone(), &state.pager_snapshot)
                }
                _ => None,
            };
            state.transition_to_browse();
            match action_opt {
                Some(action) => SettingsKeyOutcome::Action(action),
                None => {
                    tracing::error!(
                        target: "settings",
                        key = setting_key,
                        "EditingValue commit has no action_for_string arm — registry skew",
                    );
                    SettingsKeyOutcome::Changed
                }
            }
        }
        KeyCode::Backspace => {
            if cursor_byte == 0 {
                return SettingsKeyOutcome::Unchanged;
            }
            let mut new_buf = buffer.clone();
            // Find the prev char boundary.
            let prev = (0..cursor_byte)
                .rev()
                .find(|&i| new_buf.is_char_boundary(i))
                .unwrap_or(0);
            new_buf.replace_range(prev..cursor_byte, "");
            let new_cursor = prev;
            let new_error = recompute_validation(&kind_snapshot, &new_buf, state);
            update_editing_value_buffer(state, new_buf, new_cursor, new_error);
            SettingsKeyOutcome::Changed
        }
        KeyCode::Delete => {
            if cursor_byte >= buffer.len() {
                return SettingsKeyOutcome::Unchanged;
            }
            let mut new_buf = buffer.clone();
            // Find next char boundary.
            let next = (cursor_byte + 1..=new_buf.len())
                .find(|&i| new_buf.is_char_boundary(i))
                .unwrap_or(new_buf.len());
            new_buf.replace_range(cursor_byte..next, "");
            let new_error = recompute_validation(&kind_snapshot, &new_buf, state);
            update_editing_value_buffer(state, new_buf, cursor_byte, new_error);
            SettingsKeyOutcome::Changed
        }
        KeyCode::Left => {
            if cursor_byte == 0 {
                return SettingsKeyOutcome::Unchanged;
            }
            let prev = (0..cursor_byte)
                .rev()
                .find(|&i| buffer.is_char_boundary(i))
                .unwrap_or(0);
            update_editing_value_buffer(state, buffer, prev, validation_error);
            SettingsKeyOutcome::Changed
        }
        KeyCode::Right => {
            if cursor_byte >= buffer.len() {
                return SettingsKeyOutcome::Unchanged;
            }
            let next = (cursor_byte + 1..=buffer.len())
                .find(|&i| buffer.is_char_boundary(i))
                .unwrap_or(buffer.len());
            update_editing_value_buffer(state, buffer, next, validation_error);
            SettingsKeyOutcome::Changed
        }
        KeyCode::Home => {
            update_editing_value_buffer(state, buffer, 0, validation_error);
            SettingsKeyOutcome::Changed
        }
        KeyCode::End => {
            let end = buffer.len();
            update_editing_value_buffer(state, buffer, end, validation_error);
            SettingsKeyOutcome::Changed
        }
        KeyCode::Char(c) if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT => {
            // Defense-in-depth for the (currently unused) String editor path:
            // reject control + bidi/format chars so a future `String` setting
            // can't reintroduce the Trojan-Source surface.
            let accept = match &kind_snapshot {
                SettingKind::String { .. } => !crate::render::line_utils::is_unsafe_display_char(c),
                _ => false,
            };
            if !accept {
                return SettingsKeyOutcome::Unchanged;
            }
            let mut new_buf = buffer.clone();
            new_buf.insert(cursor_byte, c);
            let new_cursor = cursor_byte + c.len_utf8();
            let new_error = recompute_validation(&kind_snapshot, &new_buf, state);
            update_editing_value_buffer(state, new_buf, new_cursor, new_error);
            SettingsKeyOutcome::Changed
        }
        _ => SettingsKeyOutcome::Unchanged,
    }
}

/// Int stepper handler. Steps by range-aware small (Up/Down) or large
/// (Left/Right) deltas from [`int_step_sizes`], clamped to [min,max].
/// Non-stepper keys are rejected.
fn handle_int_stepper(
    state: &mut SettingsModalState,
    key: &KeyEvent,
    setting_key: SettingKey,
    buffer: &str,
    kind: &SettingKind,
) -> SettingsKeyOutcome {
    let SettingKind::Int { min, max, .. } = kind else {
        // Caller pre-checked the kind; defensive bail.
        return SettingsKeyOutcome::Unchanged;
    };

    let (small_step, large_step) = int_step_sizes(*min, *max);
    let step_delta = |dir: i64, large: bool| -> i64 {
        let magnitude = if large { large_step } else { small_step };
        dir * magnitude
    };

    let apply_step = |state: &mut SettingsModalState, delta: i64| -> SettingsKeyOutcome {
        let cur = buffer.parse::<i64>().unwrap_or(*min);
        let new = cur.saturating_add(delta).clamp(*min, *max);
        if new == cur {
            // Already clamped — no visible change. Report
            // Unchanged so the test for `clamps_to_min/max` can
            // distinguish a no-op from a step.
            return SettingsKeyOutcome::Unchanged;
        }
        let new_buf = new.to_string();
        let new_cursor = new_buf.len();
        update_editing_value_buffer(state, new_buf, new_cursor, None);
        SettingsKeyOutcome::Changed
    };

    // Only modifier-free or SHIFT+arrow events should trigger the
    // stepper; Ctrl/Alt/etc carry editor-unrelated semantics
    // (selection extend, history, etc) that the stepper has no
    // notion of.
    if !(key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT) {
        return SettingsKeyOutcome::Unchanged;
    }

    match key.code {
        KeyCode::Esc => {
            state.transition_to_browse();
            SettingsKeyOutcome::Changed
        }
        KeyCode::Enter => {
            // Commit. Buffer is guaranteed in-range by the
            // clamp on every step; parse + dispatch.
            let action_opt = buffer
                .parse::<i64>()
                .ok()
                .and_then(|i| action_for_int(setting_key, i));
            state.transition_to_browse();
            match action_opt {
                Some(action) => SettingsKeyOutcome::Action(action),
                None => {
                    tracing::error!(
                        target: "settings",
                        key = setting_key,
                        "Int stepper Enter has no action_for_int arm — registry skew",
                    );
                    SettingsKeyOutcome::Changed
                }
            }
        }
        // Up / k: small step up.
        KeyCode::Up | KeyCode::Char('k') => apply_step(state, step_delta(1, false)),
        // Down / j: small step down.
        KeyCode::Down | KeyCode::Char('j') => apply_step(state, step_delta(-1, false)),
        // Right / l: large step up.
        KeyCode::Right | KeyCode::Char('l') => apply_step(state, step_delta(1, true)),
        // Left / h: large step down.
        KeyCode::Left | KeyCode::Char('h') => apply_step(state, step_delta(-1, true)),
        // `d` in the Int stepper dispatches
        // `OpenResetConfirm` like Browse mode does. Close the
        // stepper first so dispatch finds `ActiveModal::Settings`
        // (the dispatch arm panics in debug mode if it sees a
        // non-Settings modal). The stepper has no `d` semantic
        // otherwise — letters are intentionally rejected — so
        // the interception is safe.
        KeyCode::Char('d') if key.modifiers.is_empty() => {
            state.transition_to_browse();
            SettingsKeyOutcome::Action(Action::OpenResetConfirm { key: setting_key })
        }
        // Everything else (digits, letters, Backspace, Delete,
        // Home, End, Tab, …) is silently ignored — the stepper is
        // not a text input.
        _ => SettingsKeyOutcome::Unchanged,
    }
}

/// Helper: rewrite the EditingValue mode payload with new buffer +
/// cursor + validation. Centralised so future variants don't need
/// to repeat the pattern-construction boilerplate.
fn update_editing_value_buffer(
    state: &mut SettingsModalState,
    buffer: String,
    cursor_byte: usize,
    validation_error: Option<String>,
) {
    let SettingsModalMode::EditingValue { key, .. } = state.mode else {
        // Caller-provided key was lost on mode shift; this is the
        // belt-and-suspenders fallback for a future refactor.
        return;
    };
    state.mode = SettingsModalMode::EditingValue {
        key,
        buffer,
        cursor_byte,
        validation_error,
    };
}

/// Helper: recompute the validation error for the current buffer
/// against the registered validator. Called on every buffer mutation
/// so the inline error indicator stays in sync.
fn recompute_validation(
    kind: &SettingKind,
    buffer: &str,
    state: &SettingsModalState,
) -> Option<String> {
    match kind {
        SettingKind::String { validator, .. } => {
            validate_string(*validator, buffer, &state.pager_snapshot.available_models)
        }
        SettingKind::Int { min, max, .. } => validate_int(buffer, *min, *max),
        _ => None,
    }
}

/// Whether `(key, canonical)` is gated off and must not be offered as a choice:
/// `permission_mode`'s "auto" when the auto gate is off, and
/// `voice_capture_mode`'s "hold" without key-release reporting. Pure (gates
/// passed as args) so it's unit-testable without touching process globals.
fn enum_choice_gated_off(
    key: SettingKey,
    canonical: &str,
    auto_mode_gate: bool,
    kitty_releases: bool,
) -> bool {
    (key == "permission_mode" && canonical == "auto" && !auto_mode_gate)
        || (key == "voice_capture_mode" && canonical == "hold" && !kitty_releases)
}

/// The effective static Enum choices for a picker, hiding gated-off options so
/// the modal never offers a choice the setter would silently no-op. Every
/// index-based picker path (len / at / render / seed) routes through this.
fn effective_enum_choices<'a>(
    key: SettingKey,
    choices: &'a [EnumChoice],
    snapshot: &PagerLocalSnapshot,
) -> Vec<&'a EnumChoice> {
    let kitty_releases = crate::app::kitty_flags_pushed();
    choices
        .iter()
        .filter(|c| {
            !enum_choice_gated_off(key, c.canonical, snapshot.auto_mode_gate, kitty_releases)
        })
        .collect()
}

/// Number of choices for the picker. Handles both
/// `SettingKind::Enum` (static catalog) and `SettingKind::DynamicEnum`
/// (catalog built from the snapshot at picker-open time).
fn picker_choices_len(state: &SettingsModalState, key: SettingKey) -> usize {
    state
        .registry
        .find(key)
        .and_then(|m| match &m.kind {
            SettingKind::Enum { choices, .. } => {
                Some(effective_enum_choices(key, choices, &state.pager_snapshot).len())
            }
            SettingKind::DynamicEnum { source, .. } => {
                Some(dynamic_enum_choices(*source, &state.pager_snapshot).len())
            }
            _ => None,
        })
        .unwrap_or(0)
}

/// Canonical value at index `idx` in the picker's choices, or `None`
/// if the key isn't a registered Enum/DynamicEnum or `idx` is out of
/// bounds.
///
/// Returns `Option<&'static str>` for static `SettingKind::Enum`
/// settings (zero allocation, since each `EnumChoice.canonical` is
/// itself `&'static str`).
fn picker_choice_at(
    state: &SettingsModalState,
    key: SettingKey,
    idx: usize,
) -> Option<&'static str> {
    let meta = state.registry.find(key)?;
    let SettingKind::Enum { choices, .. } = &meta.kind else {
        return None;
    };
    effective_enum_choices(key, choices, &state.pager_snapshot)
        .get(idx)
        .map(|c| c.canonical)
}

/// Owned-string variant of `picker_choice_at` for picker kinds whose
/// canonicals are runtime-built (`SettingKind::DynamicEnum`).
///
/// Returns the canonical at `idx` as an owned `String`. Allocates one
/// `String` per call — the picker calls this on commit + per-Up/Down
/// only when `supports_preview = true`, so the cost is bounded.
///
/// For static `SettingKind::Enum`, this also resolves correctly
/// (clones the `&'static str` into a `String`), giving the picker
/// a single unified read path when the caller doesn't need to
/// distinguish static vs. dynamic.
fn picker_choice_at_owned(
    state: &SettingsModalState,
    key: SettingKey,
    idx: usize,
) -> Option<String> {
    let meta = state.registry.find(key)?;
    match &meta.kind {
        SettingKind::Enum { choices, .. } => {
            effective_enum_choices(key, choices, &state.pager_snapshot)
                .get(idx)
                .map(|c| c.canonical.to_string())
        }
        SettingKind::DynamicEnum { source, .. } => {
            let resolved = dynamic_enum_choices(*source, &state.pager_snapshot);
            resolved.get(idx).map(|c| c.canonical.clone())
        }
        _ => None,
    }
}

/// F2 / Ctrl+, / Cmd+, are the modal-internal close keys.
///
/// Esc is intentionally NOT matched here: the `ModalWindow` chrome
/// (`views/modal_window.rs:handle_modal_key`) intercepts Esc and
/// returns `ModalWindowOutcome::CloseRequested` before this function
/// sees the event in Browse mode. `handle_filter_focused` has its own
/// Esc arm that exits filter mode without closing. Documented in the
/// module docstring.
fn is_close_key(key: &KeyEvent) -> bool {
    if key.code == KeyCode::F(2) {
        return true;
    }
    if key.code == KeyCode::Char(',')
        && (key.modifiers.contains(KeyModifiers::CONTROL)
            || key.modifiers.contains(KeyModifiers::SUPER))
    {
        return true;
    }
    false
}

fn changed_if(b: bool) -> SettingsKeyOutcome {
    if b {
        SettingsKeyOutcome::Changed
    } else {
        SettingsKeyOutcome::Unchanged
    }
}

/// Helper for `handle_settings_mouse`: when a sub-pane mouse handler
/// returns `Unchanged` but the breadcrumb hover flipped, upgrade to
/// `Changed` so the renderer repaints the breadcrumb with the new
/// fg color. Non-`Unchanged` outcomes pass through unmodified so an
/// `Action` or `Changed` from the inner handler keeps its meaning.
fn upgrade_if_breadcrumb_flipped(
    outcome: SettingsKeyOutcome,
    breadcrumb_flipped: bool,
) -> SettingsKeyOutcome {
    if breadcrumb_flipped && matches!(outcome, SettingsKeyOutcome::Unchanged) {
        SettingsKeyOutcome::Changed
    } else {
        outcome
    }
}

fn handle_browse(state: &mut SettingsModalState, key: &KeyEvent) -> SettingsKeyOutcome {
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => changed_if(state.advance_next()),
        KeyCode::Up | KeyCode::Char('k') => changed_if(state.advance_prev()),
        KeyCode::PageDown => {
            let mut moved = false;
            for _ in 0..10 {
                moved |= state.advance_next();
            }
            changed_if(moved)
        }
        KeyCode::PageUp => {
            let mut moved = false;
            for _ in 0..10 {
                moved |= state.advance_prev();
            }
            changed_if(moved)
        }
        KeyCode::Char('g') if key.modifiers.is_empty() => {
            // First selectable row IN THE FILTERED SET. When no filter
            // is active, `filtered_cache` is `(0..rows.len())` so this
            // resolves to the first row.
            let first = state
                .filtered_cache
                .iter()
                .copied()
                .find(|&idx| matches!(state.rows[idx], RowEntry::Setting { .. }))
                .unwrap_or(state.selected);
            if first != state.selected {
                state.selected = first;
                SettingsKeyOutcome::Changed
            } else {
                SettingsKeyOutcome::Unchanged
            }
        }
        KeyCode::Char('G') => {
            // Last selectable row IN THE FILTERED SET.
            let last = state
                .filtered_cache
                .iter()
                .rev()
                .copied()
                .find(|&idx| matches!(state.rows[idx], RowEntry::Setting { .. }))
                .unwrap_or(state.selected);
            if last != state.selected {
                state.selected = last;
                SettingsKeyOutcome::Changed
            } else {
                SettingsKeyOutcome::Unchanged
            }
        }
        // User-feedback follow-up: Right/`l` expands the focused
        // row's description inline; Left/`h` collapses it.
        // Expansion is per-row + persists across selection moves
        // (multiple rows can be expanded simultaneously).
        KeyCode::Right | KeyCode::Char('l') if key.modifiers.is_empty() => {
            if let Some((key, _meta)) = state.focused_setting()
                && state.expanded_keys.insert(key)
            {
                return SettingsKeyOutcome::Changed;
            }
            SettingsKeyOutcome::Unchanged
        }
        KeyCode::Left | KeyCode::Char('h') if key.modifiers.is_empty() => {
            if let Some((key, _meta)) = state.focused_setting()
                && state.expanded_keys.remove(key)
            {
                return SettingsKeyOutcome::Changed;
            }
            SettingsKeyOutcome::Unchanged
        }
        KeyCode::Char(' ') => {
            if let Some(action) = state.toggle_focused_bool() {
                SettingsKeyOutcome::Action(action)
            } else {
                SettingsKeyOutcome::Unchanged
            }
        }
        KeyCode::Enter => {
            // Group row → open its sub-sheet of child toggles.
            if state.try_enter_picking_group() {
                return SettingsKeyOutcome::Changed;
            }
            // For Bool, Enter behaves like Space (the keyboard
            // map gives both keys the toggle semantics).
            if let Some(action) = state.toggle_focused_bool() {
                return SettingsKeyOutcome::Action(action);
            }
            // Enum row → enter PickingEnum mode. The picker's chooser
            // sub-pane takes over rendering and key routing from here.
            if state.try_enter_picking_enum() {
                return SettingsKeyOutcome::Changed;
            }
            // String / Int row → enter EditingValue mode. The
            // inline editor takes over rendering and key routing.
            if state.try_enter_editing_value() {
                return SettingsKeyOutcome::Changed;
            }
            SettingsKeyOutcome::Unchanged
        }
        // `i` aliases `/` (vim-nav "press i to search").
        KeyCode::Char('/') | KeyCode::Char('i') if key.modifiers.is_empty() => {
            state.mode = SettingsModalMode::FilterFocused;
            SettingsKeyOutcome::Changed
        }
        KeyCode::Char('d') if key.modifiers.is_empty() => {
            // Reset-to-default. Resolves the focused row's
            // setting key and dispatches `Action::OpenResetConfirm`.
            // The dispatch arm boxes the SettingsModalState into the
            // `ActiveModal::ResetSettingsConfirm { settings_state, key, .. }`
            // variant so cancel returns to this exact modal state
            // (filter/scroll/selection preserved). Headers and
            // unmapped rows are no-ops — `d` only acts on a focused
            // setting row.
            //
            // We intentionally do NOT gate this on whether the
            // current value already equals the default — the
            // confirmation dialog gives the user a moment to back
            // out either way, and the dispatch arm shows an
            // "Already at default" toast on idempotent confirm.
            match state.focused_setting() {
                // Group rows have no scalar default to reset.
                Some((_, meta)) if matches!(meta.kind, SettingKind::Group { .. }) => {
                    SettingsKeyOutcome::Unchanged
                }
                Some((key, _meta)) => SettingsKeyOutcome::Action(Action::OpenResetConfirm { key }),
                // Focused row is a header (or out-of-bounds) — `d`
                // has nothing to reset. Unchanged so the user can
                // still navigate / type / etc.
                None => SettingsKeyOutcome::Unchanged,
            }
        }
        KeyCode::Backspace => {
            // Continue editing the query from Browse mode (the commit
            // path via Enter preserves the query, so Browse can be
            // entered with a non-empty query). Pop one char and
            // re-broaden the filter without switching modes. Mirrors
            // `memory_modal::handle_browse`'s Backspace arm.
            if state.query.pop().is_some() {
                state.query_cursor = state.query.len();
                state.invalidate_filter();
                state.clamp_selected_to_visible();
                SettingsKeyOutcome::Changed
            } else {
                SettingsKeyOutcome::Unchanged
            }
        }
        _ => SettingsKeyOutcome::Unchanged,
    }
}

fn handle_filter_focused(state: &mut SettingsModalState, key: &KeyEvent) -> SettingsKeyOutcome {
    match key.code {
        KeyCode::Esc => {
            state.query.clear();
            state.query_cursor = 0;
            state.invalidate_filter();
            state.clamp_selected_to_visible();
            state.transition_to_browse();
            SettingsKeyOutcome::Changed
        }
        KeyCode::Enter => {
            // Commit the filter: exit FilterFocused, return to Browse,
            // PRESERVE the query so the user can immediately Space /
            // Enter to toggle the focused (filtered) setting. This is
            // the standard TUI filter UX (fixes the
            // dead-end where Esc-clear made post-filter toggling
            // impossible without re-navigating the full list).
            state.transition_to_browse();
            SettingsKeyOutcome::Changed
        }
        KeyCode::Down => changed_if(state.advance_next()),
        KeyCode::Up => changed_if(state.advance_prev()),
        KeyCode::PageDown => {
            // Match Browse mode's fast-scroll affordance (advance 10).
            let mut moved = false;
            for _ in 0..10 {
                moved |= state.advance_next();
            }
            changed_if(moved)
        }
        KeyCode::PageUp => {
            let mut moved = false;
            for _ in 0..10 {
                moved |= state.advance_prev();
            }
            changed_if(moved)
        }
        KeyCode::Char('u') if key.modifiers == KeyModifiers::CONTROL => {
            // Clears entire query (not cursor-to-start) to match picker behavior.
            if state.query.is_empty() {
                return SettingsKeyOutcome::Unchanged;
            }
            state.query.clear();
            state.query_cursor = 0;
            state.invalidate_filter();
            state.clamp_selected_to_visible();
            SettingsKeyOutcome::Changed
        }
        KeyCode::Char(c) if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT => {
            state.query.insert(state.query_cursor, c);
            state.query_cursor += c.len_utf8();
            state.invalidate_filter();
            state.clamp_selected_to_visible();
            SettingsKeyOutcome::Changed
        }
        KeyCode::Backspace => {
            if state.query_cursor == 0 {
                return SettingsKeyOutcome::Unchanged;
            }
            let prev = state.query[..state.query_cursor]
                .char_indices()
                .next_back()
                .map_or(0, |(i, _)| i);
            state.query.drain(prev..state.query_cursor);
            state.query_cursor = prev;
            state.invalidate_filter();
            state.clamp_selected_to_visible();
            SettingsKeyOutcome::Changed
        }
        KeyCode::Left => {
            if state.query_cursor == 0 {
                return SettingsKeyOutcome::Unchanged;
            }
            state.query_cursor = state.query[..state.query_cursor]
                .char_indices()
                .next_back()
                .map_or(0, |(i, _)| i);
            SettingsKeyOutcome::Changed
        }
        KeyCode::Right => {
            if state.query_cursor >= state.query.len() {
                return SettingsKeyOutcome::Unchanged;
            }
            state.query_cursor = state.query[state.query_cursor..]
                .char_indices()
                .nth(1)
                .map_or(state.query.len(), |(i, _)| state.query_cursor + i);
            SettingsKeyOutcome::Changed
        }
        _ => SettingsKeyOutcome::Unchanged,
    }
}

// ---------------------------------------------------------------------------
// Mouse handling
// ---------------------------------------------------------------------------

/// Handle a mouse event in the modal content area.
///
/// Mirrors `memory_modal::handle_memory_mouse` for parity:
///  - Click on a row selects it; click on a Bool row toggles it.
///  - Click on the `[-]` / `[+]` adornments of an open Int editor
///    steps the value.
///  - Scroll wheel scrolls the row list by ~3 rows per tick.
///
/// **Picker short-circuit:** when the modal is in `PickingEnum`
/// mode, every mouse event is a no-op. `EditingValue` mode handles `[-]` / `[+]`
/// clicks AND treats everything else as a no-op.
pub fn handle_settings_mouse(
    state: &mut SettingsModalState,
    kind: MouseEventKind,
    column: u16,
    row: u16,
) -> SettingsKeyOutcome {
    // Clicking anywhere on the chrome
    // breadcrumb (the full `Settings › <label>` title) in a
    // sub-pane mode collapses back to Browse. Dispatched FIRST
    // so it wins over the picker / editor mouse handlers (which
    // would otherwise ignore the click as out-of-content). The
    // synthetic Esc is routed through the active sub-pane handler
    // so the same revert-preview / mode-transition logic runs as
    // for keyboard Esc — `handle_picking_enum` reverts the
    // preview action, and `handle_editing_value` just transitions
    // back.
    if matches!(
        kind,
        MouseEventKind::Down(crossterm::event::MouseButton::Left)
    ) && let Some(rect) = state.settings_breadcrumb_rect
        && rect_contains(rect, column, row)
    {
        let synthetic = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        match state.mode {
            SettingsModalMode::PickingEnum { .. } => {
                return handle_picking_enum(state, &synthetic);
            }
            SettingsModalMode::PickingGroup { .. } => {
                return handle_picking_group(state, &synthetic);
            }
            SettingsModalMode::EditingValue { .. } => {
                return handle_editing_value(state, &synthetic);
            }
            _ => {}
        }
    }

    // Track hover state for the
    // breadcrumb hit-rect so the renderer can repaint the title
    // with the brighter `accent_user` fg when the user's mouse
    // is over it. Without this cue the breadcrumb is visually
    // indistinguishable from the rest of the modal title chrome.
    // Tracked here so a hover transition is registered even when
    // the row-list / picker / editor mouse handlers below short-
    // circuit on the Moved event.
    let breadcrumb_hover_flipped = if matches!(kind, MouseEventKind::Moved) {
        let now_hovered = state
            .settings_breadcrumb_rect
            .map(|r| rect_contains(r, column, row))
            .unwrap_or(false);
        let flipped = now_hovered != state.breadcrumb_hovered;
        state.breadcrumb_hovered = now_hovered;
        // In sub-pane modes the row-list / picker / editor mouse
        // handlers don't update hover_row, so a flipped breadcrumb
        // hover is the only thing that could redraw. Force a
        // Changed outcome for those modes; in Browse the row-list
        // handler below already returns Changed when hover_row
        // moves so we let it run.
        flipped
    } else {
        false
    };

    // Handle `[-]` / `[+]` clicks
    // when in EditingValue mode AND the row is an Int. All other
    // events in EditingValue (scrolls, off-adornment clicks) are
    // no-ops.
    if matches!(state.mode, SettingsModalMode::EditingValue { .. }) {
        let outcome = handle_editor_mouse(state, kind, column, row);
        return upgrade_if_breadcrumb_flipped(outcome, breadcrumb_hover_flipped);
    }

    // PickingEnum: click-to-pick on choice rects, scroll wheel is a
    // no-op (the picker is bounded; scroll there could surprise).
    if matches!(state.mode, SettingsModalMode::PickingEnum { .. }) {
        let outcome = handle_picker_mouse(state, kind, column, row);
        return upgrade_if_breadcrumb_flipped(outcome, breadcrumb_hover_flipped);
    }

    // PickingGroup: hover tracks the child rects; a click toggles the clicked
    // child in place (same bounded-viewport, scroll-is-a-no-op contract).
    if matches!(state.mode, SettingsModalMode::PickingGroup { .. }) {
        let outcome = handle_group_mouse(state, kind, column, row);
        return upgrade_if_breadcrumb_flipped(outcome, breadcrumb_hover_flipped);
    }

    let on_list = rect_contains(state.list_area, column, row);

    // Mouse hover highlight (parity with scrollback).
    // Walks `state.row_rects` to find the row under the cursor and
    // updates `state.hover_row`. Returns early so subsequent arms
    // don't need to repeat the find. Clicks and scrolls fall
    // through to the existing arms below.
    if matches!(kind, MouseEventKind::Moved) {
        let new_hover = state
            .row_rects
            .iter()
            .position(|r| rect_contains(*r, column, row))
            .filter(|&idx| matches!(state.rows.get(idx), Some(RowEntry::Setting { .. })));
        if new_hover != state.hover_row {
            state.hover_row = new_hover;
            return SettingsKeyOutcome::Changed;
        }
        return SettingsKeyOutcome::Unchanged;
    }

    match kind {
        MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
            if !on_list {
                return SettingsKeyOutcome::Unchanged;
            }
            // Resolve the clicked row.
            let clicked_idx = state
                .row_rects
                .iter()
                .position(|r| rect_contains(*r, column, row));
            let Some(idx) = clicked_idx else {
                return SettingsKeyOutcome::Unchanged;
            };
            // Clicking a header is a no-op.
            if !matches!(state.rows[idx], RowEntry::Setting { .. }) {
                return SettingsKeyOutcome::Unchanged;
            }
            // Two-stage click semantics:
            //   - Click on a different row: only select (lets the user
            //     read the description first).
            //   - Click on the already-selected Bool row: toggle.
            //   - Click on the already-selected Enum row: open picker.
            //   - Click on the indicator cells of any Bool / Enum /
            //     String / Int row: select + activate in one click
            //     (Fitts's-law nudge — 5-col hit-rect around the
            //     small glyph).
            //
            // The per-kind dispatch is
            // collapsed into a single `if` chain — the
            // `toggle_focused_bool` / `try_enter_picking_enum` /
            // `try_enter_editing_value` helpers all return falsy
            // for non-matching kinds, so the per-kind predicates
            // are redundant.
            let row_rect = state.row_rects[idx];
            // User-feedback follow-up: col 0 of the row is the
            // `▸`/`▾` triangle glyph. A click there toggles
            // expansion (no value mutation) — matching the
            // keyboard's Right/Left arrow contract. The triangle
            // is at exactly column `row_rect.x` and is 1 cell wide.
            //
            // Two-line rows have `row_rect.height = 2`,
            // and the triangle stays on LINE 1 only — clicks on
            // line 2's col 0 (which is empty padding) shouldn't
            // toggle expansion. The y-check enforces this.
            let on_triangle = column == row_rect.x && row == row_rect.y;
            // The 5-col Fitts's-law
            // indicator hit-rect sits on the
            // value column on the right (rather than the left edge). Clicking the Bool's
            // `on`/`off` text toggles the bool in one click; clicking
            // an Enum/String/DynamicEnum/Int value opens the
            // picker/editor in one click. The hit-rect is supplied
            // by `render_setting_row` via
            // `state.value_hit_rects[idx]`.
            let value_rect = state.value_hit_rects.get(idx).copied().unwrap_or_default();
            let on_value = rect_contains(value_rect, column, row);
            let was_selected_already = state.selected == idx;
            let _ = state.select_at(idx);

            if on_triangle && let Some((key, _meta)) = state.focused_setting() {
                // Toggle expansion. Mirrors the keyboard
                // Right/Left arrow contract.
                if state.expanded_keys.contains(key) {
                    state.expanded_keys.remove(key);
                } else {
                    state.expanded_keys.insert(key);
                }
                return SettingsKeyOutcome::Changed;
            }

            if on_value || was_selected_already {
                if state.try_enter_picking_group() {
                    return SettingsKeyOutcome::Changed;
                }
                if let Some(action) = state.toggle_focused_bool() {
                    return SettingsKeyOutcome::Action(action);
                }
                if state.try_enter_picking_enum() || state.try_enter_editing_value() {
                    return SettingsKeyOutcome::Changed;
                }
            }
            // Selection moved (or was already on this row); the
            // re-render reflects the new focus.
            SettingsKeyOutcome::Changed
        }
        MouseEventKind::ScrollDown => {
            if !on_list {
                return SettingsKeyOutcome::Unchanged;
            }
            let mut moved = false;
            for _ in 0..3 {
                moved |= state.advance_next();
            }
            changed_if(moved)
        }
        MouseEventKind::ScrollUp => {
            if !on_list {
                return SettingsKeyOutcome::Unchanged;
            }
            let mut moved = false;
            for _ in 0..3 {
                moved |= state.advance_prev();
            }
            changed_if(moved)
        }
        _ => SettingsKeyOutcome::Unchanged,
    }
}

/// Handle a mouse event while the modal is in `PickingEnum` mode.
///
/// Left-click on any line of a choice's multi-line hit-rect moves
/// the picker focus to that choice (and fires the matching preview
/// dispatch, mirroring keyboard Up/Down). Clicks outside any choice
/// rect are no-ops, as are scroll wheel events (the picker viewport
/// is bounded; in-picker scrolling could surprise).
///
/// Continuation lines of a word-wrapped description share the same
/// hit-rect as the choice they belong to — clicking the second line
/// of "Opt out" picks "Opt out", same as clicking its symbol.
fn handle_picker_mouse(
    state: &mut SettingsModalState,
    kind: MouseEventKind,
    column: u16,
    row: u16,
) -> SettingsKeyOutcome {
    // Hover highlight for picker choices. Tracks the
    // choice index under the cursor in `state.hover_row` (same
    // field as the row-list path; the field is mode-aware via the
    // active `state.mode`).
    if matches!(kind, MouseEventKind::Moved) {
        let new_hover = state
            .picker_choice_rects
            .iter()
            .position(|r| r.height > 0 && rect_contains(*r, column, row));
        if new_hover != state.hover_row {
            state.hover_row = new_hover;
            return SettingsKeyOutcome::Changed;
        }
        return SettingsKeyOutcome::Unchanged;
    }

    let MouseEventKind::Down(crossterm::event::MouseButton::Left) = kind else {
        return SettingsKeyOutcome::Unchanged;
    };
    // Snapshot the picker payload under the immutable borrow.
    let (setting_key, current_idx, original_value, supports_preview) = match &state.mode {
        SettingsModalMode::PickingEnum {
            key,
            choices_idx,
            original_value,
            supports_preview,
        } => (
            *key,
            *choices_idx,
            original_value.clone(),
            *supports_preview,
        ),
        _ => return SettingsKeyOutcome::Unchanged,
    };
    let clicked_idx = state
        .picker_choice_rects
        .iter()
        .position(|r| r.height > 0 && rect_contains(*r, column, row));
    let Some(target_idx) = clicked_idx else {
        return SettingsKeyOutcome::Unchanged;
    };
    if target_idx == current_idx {
        // Already focused — re-clicking the same choice is a no-op
        // (kept for parity with the row-list's "already-focused
        // click commits" semantics; commit fires on Enter, not on
        // a re-click).
        return SettingsKeyOutcome::Unchanged;
    }
    // Reuse the keyboard nav helper to update `choices_idx` AND
    // fire the matching preview Action (when the kind supports it).
    set_picker_idx(
        state,
        setting_key,
        target_idx,
        original_value,
        supports_preview,
    )
}

/// Handle a mouse event while the modal is in `PickingGroup` mode.
///
/// Hover tracks the child row under the cursor; a left-click moves focus to the
/// clicked child AND toggles it in one click (toggles, unlike the enum picker's
/// commit-on-Enter). Scroll wheel is a no-op (the sub-sheet is bounded).
fn handle_group_mouse(
    state: &mut SettingsModalState,
    kind: MouseEventKind,
    column: u16,
    row: u16,
) -> SettingsKeyOutcome {
    if matches!(kind, MouseEventKind::Moved) {
        let new_hover = state
            .picker_choice_rects
            .iter()
            .position(|r| r.height > 0 && rect_contains(*r, column, row));
        if new_hover != state.hover_row {
            state.hover_row = new_hover;
            return SettingsKeyOutcome::Changed;
        }
        return SettingsKeyOutcome::Unchanged;
    }
    let MouseEventKind::Down(crossterm::event::MouseButton::Left) = kind else {
        return SettingsKeyOutcome::Unchanged;
    };
    let group_key = match &state.mode {
        SettingsModalMode::PickingGroup { key, .. } => *key,
        _ => return SettingsKeyOutcome::Unchanged,
    };
    let children = group_children(state, group_key);
    let clicked_idx = state
        .picker_choice_rects
        .iter()
        .position(|r| r.height > 0 && rect_contains(*r, column, row));
    let Some(idx) = clicked_idx else {
        return SettingsKeyOutcome::Unchanged;
    };
    state.mode = SettingsModalMode::PickingGroup {
        key: group_key,
        child_idx: idx,
    };
    let Some(child_key) = children.get(idx).copied() else {
        return SettingsKeyOutcome::Changed;
    };
    let cur = matches!(state.value_for(child_key), Some(SettingValue::Bool(true)));
    match action_for_bool(child_key, !cur) {
        Some(action) => SettingsKeyOutcome::Action(action),
        None => SettingsKeyOutcome::Changed,
    }
}

/// Handle a mouse event while in `EditingValue` mode. Dispatches
/// clicks on the Int editor's `[-]` / `[+]` adornments as
/// keyboard-equivalent Down/Up steps; everything else is a no-op.
fn handle_editor_mouse(
    state: &mut SettingsModalState,
    kind: MouseEventKind,
    column: u16,
    row: u16,
) -> SettingsKeyOutcome {
    let MouseEventKind::Down(crossterm::event::MouseButton::Left) = kind else {
        return SettingsKeyOutcome::Unchanged;
    };
    let (dec_rect, inc_rect) = state.editor_adornment_rects;
    let step_dir = if rect_contains(dec_rect, column, row) {
        StepDir::Down
    } else if rect_contains(inc_rect, column, row) {
        StepDir::Up
    } else {
        return SettingsKeyOutcome::Unchanged;
    };
    // Synthesize the equivalent keyboard event so the actual
    // step + clamp + validation logic lives in one place
    // (`handle_editing_value`). Mirrors the picker's choice-click
    // approach.
    let synthetic = KeyEvent::new(
        match step_dir {
            StepDir::Up => KeyCode::Up,
            StepDir::Down => KeyCode::Down,
        },
        KeyModifiers::NONE,
    );
    handle_editing_value(state, &synthetic)
}

/// Direction tag for the Int editor's spinner mouse-click → key
/// synthesis. Internal to `handle_editor_mouse`.
#[derive(Clone, Copy)]
enum StepDir {
    Up,
    Down,
}

fn rect_contains(r: Rect, column: u16, row: u16) -> bool {
    r.width > 0
        && r.height > 0
        && column >= r.x
        && column < r.x.saturating_add(r.width)
        && row >= r.y
        && row < r.y.saturating_add(r.height)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "settings_modal_tests.rs"]
mod tests;
