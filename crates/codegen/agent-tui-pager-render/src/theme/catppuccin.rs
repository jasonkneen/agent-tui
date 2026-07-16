//! Catppuccin Mocha theme.
//!
//! Official Catppuccin Mocha palette (https://github.com/catppuccin/catppuccin).

use ratatui::style::{Color, Modifier};

use super::tokyonight::Theme;

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

#[allow(dead_code)]
mod palette {
    use super::*;

    pub const BASE: Color = rgb(30, 30, 46); // #1e1e2e
    pub const MANTLE: Color = rgb(24, 24, 37); // #181825
    pub const CRUST: Color = rgb(17, 17, 27); // #11111b
    pub const SURFACE0: Color = rgb(49, 50, 68); // #313244
    pub const SURFACE1: Color = rgb(69, 71, 90); // #45475a
    pub const SURFACE2: Color = rgb(88, 91, 112); // #585b70
    pub const OVERLAY0: Color = rgb(108, 112, 134); // #6c7086
    pub const OVERLAY1: Color = rgb(127, 132, 156); // #7f849c
    pub const OVERLAY2: Color = rgb(147, 153, 178); // #9399b2
    pub const TEXT: Color = rgb(205, 214, 244); // #cdd6f4
    pub const SUBTEXT0: Color = rgb(166, 173, 200); // #a6adc8
    pub const SUBTEXT1: Color = rgb(186, 194, 222); // #bac2de

    pub const ROSEWATER: Color = rgb(245, 224, 220); // #f5e0dc
    pub const FLAMINGO: Color = rgb(242, 205, 205); // #f2cdcd
    pub const PINK: Color = rgb(245, 194, 231); // #f5c2e7
    pub const MAUVE: Color = rgb(203, 166, 247); // #cba6f7
    pub const RED: Color = rgb(243, 139, 168); // #f38ba8
    pub const MAROON: Color = rgb(235, 160, 172); // #eba0ac
    pub const PEACH: Color = rgb(250, 179, 135); // #fab387
    pub const YELLOW: Color = rgb(249, 226, 175); // #f9e2af
    pub const GREEN: Color = rgb(166, 227, 161); // #a6e3a1
    pub const TEAL: Color = rgb(148, 226, 213); // #94e2d5
    pub const SKY: Color = rgb(137, 220, 235); // #89dceb
    pub const SAPPHIRE: Color = rgb(116, 199, 236); // #74c7ec
    pub const BLUE: Color = rgb(137, 180, 250); // #89b4fa
    pub const LAVENDER: Color = rgb(180, 190, 254); // #b4befe
}
use palette::*;

impl Theme {
    pub const fn catppuccin() -> Self {
        Self {
            bg_base: BASE,
            bg_light: SURFACE0,
            bg_dark: MANTLE,
            bg_highlight: SURFACE0,
            bg_hover: SURFACE1,
            bg_terminal: CRUST,

            accent_user: LAVENDER,
            accent_assistant: MAUVE,
            accent_thinking: OVERLAY1,
            accent_tool: OVERLAY2,
            accent_system: BLUE,
            accent_error: RED,
            accent_success: GREEN,
            accent_running: PINK,
            accent_skill: TEAL,

            text_primary: TEXT,
            text_secondary: SUBTEXT1,

            gray_dim: SURFACE2,
            gray: OVERLAY2,
            gray_bright: SUBTEXT0,

            command: YELLOW,
            path: PEACH,
            running: SKY,
            warning: YELLOW,

            fuzzy_accent: MAUVE,

            accent_plan: YELLOW,
            accent_verify: MAUVE,
            accent_feedback: TEAL,
            accent_remember: GREEN,

            selection_border: SURFACE2,
            hover_border: SURFACE1,
            prompt_border: SURFACE1,
            prompt_border_active: MAUVE,

            accent_model: SAPPHIRE,

            scrollbar_bg: MANTLE,
            scrollbar_fg: SURFACE2,

            diff_delete_bg: rgb(60, 42, 50),
            diff_delete_fg: RED,
            diff_insert_bg: rgb(36, 49, 43),
            diff_insert_fg: GREEN,
            diff_equal_fg: OVERLAY2,
            diff_gutter_fg: OVERLAY2,

            bg_visual: SURFACE1,

            paste_bg: MANTLE,
            paste_fg: SUBTEXT0,
            paste_dim: OVERLAY2,

            md_heading_h1: TEXT,
            md_heading_h1_mod: Modifier::BOLD,
            md_heading_h2: MAUVE,
            md_heading_h2_mod: Modifier::BOLD,
            md_heading_h3: BLUE,
            md_heading_h3_mod: Modifier::BOLD,
            md_heading_h4: PINK,
            md_heading_h4_mod: Modifier::BOLD.union(Modifier::ITALIC),
            md_heading_h5: PEACH,
            md_heading_h5_mod: Modifier::BOLD,
            md_heading_h6: TEAL,
            md_heading_h6_mod: Modifier::BOLD,
            md_code: SKY,
            md_task_checked: GREEN,
            md_task_unchecked: SUBTEXT0,
            md_muted: OVERLAY2,
            md_code_bg: MANTLE,
            md_text: TEXT,
            link_fg: BLUE,
        }
    }
}
