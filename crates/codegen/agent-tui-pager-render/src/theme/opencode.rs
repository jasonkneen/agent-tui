//! OpenCode default theme — warm orange primary on near-black grays.
//!
//! Palette sourced from OpenCode TUI `opencode` theme (v1.x):
//! primary `#fab283`, secondary `#5c9cf5`, accent `#9d7cd8`.

use ratatui::style::{Color, Modifier};

use super::tokyonight::Theme;

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

#[allow(dead_code)]
mod palette {
    use super::*;

    // Backgrounds (darkStep1–5)
    pub const BG: Color = rgb(10, 10, 10); // #0a0a0a
    pub const PANEL: Color = rgb(20, 20, 20); // #141414
    pub const ELEMENT: Color = rgb(30, 30, 30); // #1e1e1e
    pub const ELEVATED: Color = rgb(40, 40, 40); // #282828
    pub const HOVER: Color = rgb(50, 50, 50); // #323232

    // Borders / muted
    pub const BORDER: Color = rgb(72, 72, 72); // #484848
    pub const BORDER_ACTIVE: Color = rgb(96, 96, 96); // #606060
    pub const MUTED: Color = rgb(128, 128, 128); // #808080

    // Text
    pub const TEXT: Color = rgb(238, 238, 238); // #eeeeee
    pub const TEXT_DIM: Color = rgb(200, 200, 200); // #c8c8c8

    // Accents
    pub const PRIMARY: Color = rgb(250, 178, 131); // #fab283 warm orange
    pub const PRIMARY_BRIGHT: Color = rgb(255, 192, 159); // #ffc09f
    pub const SECONDARY: Color = rgb(92, 156, 245); // #5c9cf5 blue
    pub const ACCENT: Color = rgb(157, 124, 216); // #9d7cd8 purple
    pub const RED: Color = rgb(224, 108, 117); // #e06c75
    pub const ORANGE: Color = rgb(245, 167, 66); // #f5a742
    pub const GREEN: Color = rgb(127, 216, 143); // #7fd88f
    pub const CYAN: Color = rgb(86, 182, 194); // #56b6c2
    pub const YELLOW: Color = rgb(229, 192, 123); // #e5c07b
}
use palette::*;

impl Theme {
    pub const fn opencode() -> Self {
        Self {
            bg_base: BG,
            bg_light: ELEVATED,
            bg_dark: PANEL,
            bg_highlight: ELEMENT,
            bg_hover: HOVER,
            bg_terminal: BG,

            accent_user: PRIMARY_BRIGHT,
            accent_assistant: ACCENT,
            accent_thinking: MUTED,
            accent_tool: BORDER_ACTIVE,
            accent_system: SECONDARY,
            accent_error: RED,
            accent_success: GREEN,
            accent_running: PRIMARY,
            accent_skill: ACCENT,

            text_primary: TEXT,
            text_secondary: TEXT_DIM,

            gray_dim: BORDER,
            gray: MUTED,
            gray_bright: TEXT_DIM,

            command: YELLOW,
            path: ORANGE,
            running: CYAN,
            warning: ORANGE,

            fuzzy_accent: PRIMARY,

            accent_plan: YELLOW,
            accent_verify: ACCENT,
            accent_feedback: GREEN,
            accent_remember: GREEN,

            selection_border: BORDER_ACTIVE,
            hover_border: BORDER,
            prompt_border: BORDER,
            prompt_border_active: PRIMARY,

            accent_model: SECONDARY,

            scrollbar_bg: PANEL,
            scrollbar_fg: BORDER_ACTIVE,

            diff_delete_bg: rgb(55, 34, 44),
            diff_delete_fg: RED,
            diff_insert_bg: rgb(32, 48, 59),
            diff_insert_fg: GREEN,
            diff_equal_fg: MUTED,
            diff_gutter_fg: MUTED,

            bg_visual: HOVER,

            paste_bg: PANEL,
            paste_fg: TEXT_DIM,
            paste_dim: MUTED,

            md_heading_h1: TEXT,
            md_heading_h1_mod: Modifier::BOLD,
            md_heading_h2: ACCENT,
            md_heading_h2_mod: Modifier::BOLD,
            md_heading_h3: PRIMARY,
            md_heading_h3_mod: Modifier::BOLD,
            md_heading_h4: SECONDARY,
            md_heading_h4_mod: Modifier::BOLD.union(Modifier::ITALIC),
            md_heading_h5: YELLOW,
            md_heading_h5_mod: Modifier::BOLD,
            md_heading_h6: CYAN,
            md_heading_h6_mod: Modifier::BOLD,
            md_code: CYAN,
            md_task_checked: GREEN,
            md_task_unchecked: TEXT_DIM,
            md_muted: MUTED,
            md_code_bg: PANEL,
            md_text: TEXT,
            link_fg: PRIMARY,
        }
    }
}
