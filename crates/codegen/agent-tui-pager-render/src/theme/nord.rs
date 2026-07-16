//! Nord theme — Arctic, north-bluish color palette.
//!
//! Official Nord palette (https://www.nordtheme.com/).

use ratatui::style::{Color, Modifier};

use super::tokyonight::Theme;

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

#[allow(dead_code)]
mod palette {
    use super::*;

    pub const NORD0: Color = rgb(46, 52, 64); // #2E3440
    pub const NORD1: Color = rgb(59, 66, 82); // #3B4252
    pub const NORD2: Color = rgb(67, 76, 94); // #434C5E
    pub const NORD3: Color = rgb(76, 86, 106); // #4C566A
    pub const NORD4: Color = rgb(216, 222, 233); // #D8DEE9
    pub const NORD5: Color = rgb(229, 233, 240); // #E5E9F0
    pub const NORD6: Color = rgb(236, 239, 244); // #ECEFF4
    pub const NORD7: Color = rgb(143, 188, 187); // #8FBCBB
    pub const NORD8: Color = rgb(136, 192, 208); // #88C0D0
    pub const NORD9: Color = rgb(129, 161, 193); // #81A1C1
    pub const NORD10: Color = rgb(94, 129, 172); // #5E81AC
    pub const NORD11: Color = rgb(191, 97, 106); // #BF616A
    pub const NORD12: Color = rgb(208, 135, 112); // #D08770
    pub const NORD13: Color = rgb(235, 203, 139); // #EBCB8B
    pub const NORD14: Color = rgb(163, 190, 140); // #A3BE8C
    pub const NORD15: Color = rgb(180, 142, 173); // #B48EAD

    pub const MUTED: Color = rgb(139, 149, 167); // #8B95A7
}
use palette::*;

impl Theme {
    pub const fn nord() -> Self {
        Self {
            bg_base: NORD0,
            bg_light: NORD2,
            bg_dark: NORD1,
            bg_highlight: NORD1,
            bg_hover: NORD3,
            bg_terminal: NORD0,

            accent_user: NORD6,
            accent_assistant: NORD8,
            accent_thinking: NORD3,
            accent_tool: MUTED,
            accent_system: NORD9,
            accent_error: NORD11,
            accent_success: NORD14,
            accent_running: NORD7,
            accent_skill: NORD15,

            text_primary: NORD6,
            text_secondary: NORD4,

            gray_dim: NORD3,
            gray: MUTED,
            gray_bright: NORD4,

            command: NORD13,
            path: NORD12,
            running: NORD8,
            warning: NORD12,

            fuzzy_accent: NORD8,

            accent_plan: NORD13,
            accent_verify: NORD15,
            accent_feedback: NORD7,
            accent_remember: NORD14,

            selection_border: NORD3,
            hover_border: NORD2,
            prompt_border: NORD2,
            prompt_border_active: NORD8,

            accent_model: NORD9,

            scrollbar_bg: NORD1,
            scrollbar_fg: NORD3,

            diff_delete_bg: rgb(59, 50, 55),
            diff_delete_fg: NORD11,
            diff_insert_bg: rgb(50, 58, 52),
            diff_insert_fg: NORD14,
            diff_equal_fg: MUTED,
            diff_gutter_fg: MUTED,

            bg_visual: NORD2,

            paste_bg: NORD1,
            paste_fg: NORD4,
            paste_dim: MUTED,

            md_heading_h1: NORD6,
            md_heading_h1_mod: Modifier::BOLD,
            md_heading_h2: NORD8,
            md_heading_h2_mod: Modifier::BOLD,
            md_heading_h3: NORD9,
            md_heading_h3_mod: Modifier::BOLD,
            md_heading_h4: NORD7,
            md_heading_h4_mod: Modifier::BOLD.union(Modifier::ITALIC),
            md_heading_h5: NORD13,
            md_heading_h5_mod: Modifier::BOLD,
            md_heading_h6: NORD15,
            md_heading_h6_mod: Modifier::BOLD,
            md_code: NORD7,
            md_task_checked: NORD14,
            md_task_unchecked: NORD4,
            md_muted: MUTED,
            md_code_bg: NORD1,
            md_text: NORD4,
            link_fg: NORD9,
        }
    }
}
