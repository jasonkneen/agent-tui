//! Dracula theme.
//!
//! Official Dracula palette (https://draculatheme.com/).

use ratatui::style::{Color, Modifier};

use super::tokyonight::Theme;

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

#[allow(dead_code)]
mod palette {
    use super::*;

    pub const BACKGROUND: Color = rgb(40, 42, 54); // #282a36
    pub const CURRENT_LINE: Color = rgb(68, 71, 90); // #44475a
    pub const SELECTION: Color = rgb(68, 71, 90); // #44475a
    pub const FOREGROUND: Color = rgb(248, 248, 242); // #f8f8f2
    pub const COMMENT: Color = rgb(98, 114, 164); // #6272a4
    pub const CYAN: Color = rgb(139, 233, 253); // #8be9fd
    pub const GREEN: Color = rgb(80, 250, 123); // #50fa7b
    pub const ORANGE: Color = rgb(255, 184, 108); // #ffb86c
    pub const PINK: Color = rgb(255, 121, 198); // #ff79c6
    pub const PURPLE: Color = rgb(189, 147, 249); // #bd93f9
    pub const RED: Color = rgb(255, 85, 85); // #ff5555
    pub const YELLOW: Color = rgb(241, 250, 140); // #f1fa8c

    pub const BG_DARK: Color = rgb(33, 34, 44); // #21222c
    pub const BG_DARKER: Color = rgb(25, 26, 33); // #191a21
}
use palette::*;

impl Theme {
    pub const fn dracula() -> Self {
        Self {
            bg_base: BACKGROUND,
            bg_light: CURRENT_LINE,
            bg_dark: BG_DARK,
            bg_highlight: CURRENT_LINE,
            bg_hover: rgb(80, 84, 108),
            bg_terminal: BG_DARKER,

            accent_user: FOREGROUND,
            accent_assistant: PURPLE,
            accent_thinking: COMMENT,
            accent_tool: COMMENT,
            accent_system: CYAN,
            accent_error: RED,
            accent_success: GREEN,
            accent_running: PINK,
            accent_skill: ORANGE,

            text_primary: FOREGROUND,
            text_secondary: rgb(200, 200, 210),

            gray_dim: BG_DARKER,
            gray: COMMENT,
            gray_bright: rgb(152, 154, 164),

            command: YELLOW,
            path: ORANGE,
            running: CYAN,
            warning: YELLOW,

            fuzzy_accent: PINK,

            accent_plan: YELLOW,
            accent_verify: PURPLE,
            accent_feedback: GREEN,
            accent_remember: GREEN,

            selection_border: PURPLE,
            hover_border: SELECTION,
            prompt_border: SELECTION,
            prompt_border_active: PINK,

            accent_model: CYAN,

            scrollbar_bg: BG_DARK,
            scrollbar_fg: CURRENT_LINE,

            diff_delete_bg: rgb(58, 26, 26),
            diff_delete_fg: RED,
            diff_insert_bg: rgb(26, 58, 26),
            diff_insert_fg: GREEN,
            diff_equal_fg: COMMENT,
            diff_gutter_fg: COMMENT,

            bg_visual: SELECTION,

            paste_bg: BG_DARK,
            paste_fg: rgb(200, 200, 210),
            paste_dim: COMMENT,

            md_heading_h1: FOREGROUND,
            md_heading_h1_mod: Modifier::BOLD,
            md_heading_h2: PURPLE,
            md_heading_h2_mod: Modifier::BOLD,
            md_heading_h3: PINK,
            md_heading_h3_mod: Modifier::BOLD,
            md_heading_h4: CYAN,
            md_heading_h4_mod: Modifier::BOLD.union(Modifier::ITALIC),
            md_heading_h5: ORANGE,
            md_heading_h5_mod: Modifier::BOLD,
            md_heading_h6: GREEN,
            md_heading_h6_mod: Modifier::BOLD,
            md_code: CYAN,
            md_task_checked: GREEN,
            md_task_unchecked: rgb(200, 200, 210),
            md_muted: COMMENT,
            md_code_bg: BG_DARK,
            md_text: FOREGROUND,
            link_fg: CYAN,
        }
    }
}
