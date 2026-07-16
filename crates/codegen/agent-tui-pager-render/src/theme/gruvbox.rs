//! Gruvbox dark theme.
//!
//! Official Gruvbox dark hard-ish palette (https://github.com/morhetz/gruvbox).

use ratatui::style::{Color, Modifier};

use super::tokyonight::Theme;

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

#[allow(dead_code)]
mod palette {
    use super::*;

    pub const BG0: Color = rgb(40, 40, 40); // #282828
    pub const BG1: Color = rgb(60, 56, 54); // #3c3836
    pub const BG2: Color = rgb(80, 73, 69); // #504945
    pub const BG3: Color = rgb(102, 92, 84); // #665c54
    pub const FG0: Color = rgb(251, 241, 199); // #fbf1c7
    pub const FG1: Color = rgb(235, 219, 178); // #ebdbb2
    pub const GRAY: Color = rgb(146, 131, 116); // #928374

    pub const RED: Color = rgb(251, 73, 52); // #fb4934 bright
    pub const GREEN: Color = rgb(184, 187, 38); // #b8bb26 bright
    pub const YELLOW: Color = rgb(250, 189, 47); // #fabd2f bright
    pub const BLUE: Color = rgb(131, 165, 152); // #83a598 bright
    pub const PURPLE: Color = rgb(211, 134, 155); // #d3869b bright
    pub const AQUA: Color = rgb(142, 192, 124); // #8ec07c bright
    pub const ORANGE: Color = rgb(254, 128, 25); // #fe8019 bright
}
use palette::*;

impl Theme {
    pub const fn gruvbox() -> Self {
        Self {
            bg_base: BG0,
            bg_light: BG2,
            bg_dark: BG1,
            bg_highlight: BG1,
            bg_hover: BG2,
            bg_terminal: BG0,

            accent_user: FG0,
            accent_assistant: PURPLE,
            accent_thinking: GRAY,
            accent_tool: BG3,
            accent_system: BLUE,
            accent_error: RED,
            accent_success: GREEN,
            accent_running: AQUA,
            accent_skill: ORANGE,

            text_primary: FG1,
            text_secondary: FG0,

            gray_dim: BG3,
            gray: GRAY,
            gray_bright: FG0,

            command: YELLOW,
            path: ORANGE,
            running: AQUA,
            warning: YELLOW,

            fuzzy_accent: ORANGE,

            accent_plan: YELLOW,
            accent_verify: PURPLE,
            accent_feedback: AQUA,
            accent_remember: GREEN,

            selection_border: BG3,
            hover_border: BG2,
            prompt_border: BG2,
            prompt_border_active: ORANGE,

            accent_model: BLUE,

            scrollbar_bg: BG1,
            scrollbar_fg: BG3,

            diff_delete_bg: rgb(60, 30, 28),
            diff_delete_fg: RED,
            diff_insert_bg: rgb(40, 48, 24),
            diff_insert_fg: GREEN,
            diff_equal_fg: GRAY,
            diff_gutter_fg: GRAY,

            bg_visual: BG2,

            paste_bg: BG1,
            paste_fg: FG0,
            paste_dim: GRAY,

            md_heading_h1: FG0,
            md_heading_h1_mod: Modifier::BOLD,
            md_heading_h2: ORANGE,
            md_heading_h2_mod: Modifier::BOLD,
            md_heading_h3: YELLOW,
            md_heading_h3_mod: Modifier::BOLD,
            md_heading_h4: AQUA,
            md_heading_h4_mod: Modifier::BOLD.union(Modifier::ITALIC),
            md_heading_h5: PURPLE,
            md_heading_h5_mod: Modifier::BOLD,
            md_heading_h6: BLUE,
            md_heading_h6_mod: Modifier::BOLD,
            md_code: AQUA,
            md_task_checked: GREEN,
            md_task_unchecked: FG0,
            md_muted: GRAY,
            md_code_bg: BG1,
            md_text: FG1,
            link_fg: BLUE,
        }
    }
}
