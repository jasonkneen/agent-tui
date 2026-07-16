//! NERV theme — Evangelion Unit-01 inspired palette.
//!
//! Near-black cockpit backgrounds with Unit-01 purple, neon green
//! stripes, warning orange, and NERV red. Distinctive truecolor theme.

use ratatui::style::{Color, Modifier};

use super::tokyonight::Theme;

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

#[allow(dead_code)]
mod palette {
    use super::*;

    // Cockpit blacks with a faint purple cast
    pub const BG: Color = rgb(8, 6, 12); // #08060c
    pub const SURFACE: Color = rgb(14, 10, 20); // #0e0a14
    pub const PANEL: Color = rgb(22, 16, 32); // #161020
    pub const ELEVATED: Color = rgb(34, 24, 48); // #221830
    pub const HOVER: Color = rgb(48, 34, 68); // #302244

    // Unit-01 purple ramp
    pub const PURPLE: Color = rgb(118, 88, 152); // #765898
    pub const PURPLE_BRIGHT: Color = rgb(168, 130, 220); // #a882dc
    pub const PURPLE_DIM: Color = rgb(78, 56, 108); // #4e386c

    // Neon green (Unit-01 stripe / entry plug)
    pub const GREEN: Color = rgb(82, 208, 83); // #52d053
    pub const GREEN_DIM: Color = rgb(50, 140, 70); // #328c46
    pub const GREEN_BRIGHT: Color = rgb(140, 255, 150); // #8cff96

    // NERV warning / Unit-02 orange-red
    pub const ORANGE: Color = rgb(230, 119, 11); // #e6770b
    pub const RED: Color = rgb(211, 41, 15); // #d3290f
    pub const RED_BRIGHT: Color = rgb(255, 70, 50); // #ff4632
    pub const GOLD: Color = rgb(240, 190, 60); // #f0be3c

    // Text
    pub const TEXT: Color = rgb(230, 225, 240); // #e6e1f0
    pub const TEXT_DIM: Color = rgb(180, 170, 200); // #b4aac8
    pub const MUTED: Color = rgb(110, 100, 130); // #6e6482
    pub const SUBTLE: Color = rgb(70, 60, 90); // #463c5a

    // Interface cyan (MAGI / status displays)
    pub const CYAN: Color = rgb(80, 200, 220); // #50c8dc
}
use palette::*;

impl Theme {
    pub const fn nerv() -> Self {
        Self {
            bg_base: BG,
            bg_light: ELEVATED,
            bg_dark: SURFACE,
            bg_highlight: PANEL,
            bg_hover: HOVER,
            bg_terminal: BG,

            accent_user: GREEN_BRIGHT,
            accent_assistant: PURPLE_BRIGHT,
            accent_thinking: MUTED,
            accent_tool: SUBTLE,
            accent_system: CYAN,
            accent_error: RED_BRIGHT,
            accent_success: GREEN,
            accent_running: ORANGE,
            accent_skill: PURPLE,

            text_primary: TEXT,
            text_secondary: TEXT_DIM,

            gray_dim: SUBTLE,
            gray: MUTED,
            gray_bright: TEXT_DIM,

            command: GOLD,
            path: ORANGE,
            running: CYAN,
            warning: ORANGE,

            fuzzy_accent: GREEN,

            accent_plan: GOLD,
            accent_verify: PURPLE_BRIGHT,
            accent_feedback: GREEN,
            accent_remember: GREEN_DIM,

            selection_border: PURPLE,
            hover_border: PURPLE_DIM,
            prompt_border: PURPLE_DIM,
            prompt_border_active: GREEN,

            accent_model: CYAN,

            // Track dark purple; thumb bright enough to survive follow-mode blend
            scrollbar_bg: SURFACE,
            scrollbar_fg: PURPLE,

            diff_delete_bg: rgb(50, 12, 10),
            diff_delete_fg: RED_BRIGHT,
            diff_insert_bg: rgb(10, 40, 18),
            diff_insert_fg: GREEN,
            diff_equal_fg: MUTED,
            diff_gutter_fg: MUTED,

            bg_visual: HOVER,

            paste_bg: SURFACE,
            paste_fg: TEXT_DIM,
            paste_dim: MUTED,

            md_heading_h1: TEXT,
            md_heading_h1_mod: Modifier::BOLD,
            md_heading_h2: GREEN_BRIGHT,
            md_heading_h2_mod: Modifier::BOLD,
            md_heading_h3: PURPLE_BRIGHT,
            md_heading_h3_mod: Modifier::BOLD,
            md_heading_h4: ORANGE,
            md_heading_h4_mod: Modifier::BOLD.union(Modifier::ITALIC),
            md_heading_h5: GOLD,
            md_heading_h5_mod: Modifier::BOLD,
            md_heading_h6: CYAN,
            md_heading_h6_mod: Modifier::BOLD,
            md_code: GREEN,
            md_task_checked: GREEN,
            md_task_unchecked: TEXT_DIM,
            md_muted: MUTED,
            md_code_bg: SURFACE,
            md_text: TEXT,
            link_fg: CYAN,
        }
    }
}
