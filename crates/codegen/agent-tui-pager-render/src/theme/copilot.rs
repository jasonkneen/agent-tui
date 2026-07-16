//! GitHub Copilot theme — GitHub Dark canvas with Copilot purple accents.
//!
//! Backgrounds/text from Primer GitHub Dark; brand purples from the
//! GitHub Copilot brand toolkit (`#8534F3`, `#C898FD`, `#B870FF`).

use ratatui::style::{Color, Modifier};

use super::tokyonight::Theme;

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

#[allow(dead_code)]
mod palette {
    use super::*;

    // GitHub Dark
    pub const BG: Color = rgb(13, 17, 23); // #0d1117
    pub const BG_ALT: Color = rgb(1, 4, 9); // #010409
    pub const BG_PANEL: Color = rgb(22, 27, 34); // #161b22
    pub const BG_HOVER: Color = rgb(33, 38, 45); // #21262d
    pub const BORDER: Color = rgb(48, 54, 61); // #30363d
    pub const FG: Color = rgb(201, 209, 217); // #c9d1d9
    pub const FG_MUTED: Color = rgb(139, 148, 158); // #8b949e

    // Semantic (GitHub)
    pub const BLUE: Color = rgb(88, 166, 255); // #58a6ff
    pub const GREEN: Color = rgb(63, 185, 80); // #3fb950
    pub const RED: Color = rgb(248, 81, 73); // #f85149
    pub const ORANGE: Color = rgb(210, 153, 34); // #d29922
    pub const YELLOW: Color = rgb(227, 179, 65); // #e3b341
    pub const CYAN: Color = rgb(57, 197, 207); // #39c5cf
    pub const PINK: Color = rgb(255, 123, 114); // #ff7b72

    // Copilot brand purples
    pub const COPILOT: Color = rgb(133, 52, 243); // #8534F3
    pub const COPILOT_SOFT: Color = rgb(200, 152, 253); // #C898FD
    pub const COPILOT_MID: Color = rgb(184, 112, 255); // #B870FF
    pub const COPILOT_DEEP: Color = rgb(67, 23, 158); // #43179E
}
use palette::*;

impl Theme {
    pub const fn copilot() -> Self {
        Self {
            bg_base: BG,
            bg_light: BG_HOVER,
            bg_dark: BG_PANEL,
            bg_highlight: BG_PANEL,
            bg_hover: BORDER,
            bg_terminal: BG_ALT,

            accent_user: COPILOT_SOFT,
            accent_assistant: COPILOT_MID,
            accent_thinking: FG_MUTED,
            accent_tool: FG_MUTED,
            accent_system: BLUE,
            accent_error: RED,
            accent_success: GREEN,
            accent_running: COPILOT,
            accent_skill: COPILOT_MID,

            text_primary: FG,
            text_secondary: FG_MUTED,

            gray_dim: BORDER,
            gray: FG_MUTED,
            gray_bright: rgb(177, 186, 196), // #b1bac4

            command: YELLOW,
            path: ORANGE,
            running: CYAN,
            warning: YELLOW,

            fuzzy_accent: COPILOT_SOFT,

            accent_plan: YELLOW,
            accent_verify: COPILOT_MID,
            accent_feedback: GREEN,
            accent_remember: GREEN,

            selection_border: COPILOT,
            hover_border: BORDER,
            prompt_border: BORDER,
            prompt_border_active: COPILOT_MID,

            accent_model: BLUE,

            scrollbar_bg: BG_PANEL,
            scrollbar_fg: BORDER,

            diff_delete_bg: rgb(103, 6, 12),
            diff_delete_fg: RED,
            diff_insert_bg: rgb(3, 58, 22),
            diff_insert_fg: GREEN,
            diff_equal_fg: FG_MUTED,
            diff_gutter_fg: FG_MUTED,

            bg_visual: BG_HOVER,

            paste_bg: BG_PANEL,
            paste_fg: FG_MUTED,
            paste_dim: FG_MUTED,

            md_heading_h1: FG,
            md_heading_h1_mod: Modifier::BOLD,
            md_heading_h2: COPILOT_SOFT,
            md_heading_h2_mod: Modifier::BOLD,
            md_heading_h3: COPILOT_MID,
            md_heading_h3_mod: Modifier::BOLD,
            md_heading_h4: BLUE,
            md_heading_h4_mod: Modifier::BOLD.union(Modifier::ITALIC),
            md_heading_h5: YELLOW,
            md_heading_h5_mod: Modifier::BOLD,
            md_heading_h6: CYAN,
            md_heading_h6_mod: Modifier::BOLD,
            md_code: COPILOT_SOFT,
            md_task_checked: GREEN,
            md_task_unchecked: FG_MUTED,
            md_muted: FG_MUTED,
            md_code_bg: BG_PANEL,
            md_text: FG,
            link_fg: BLUE,
        }
    }
}
