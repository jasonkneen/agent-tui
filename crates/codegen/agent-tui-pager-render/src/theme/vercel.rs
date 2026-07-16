//! Vercel / Geist dark theme — monochrome ink with blue accents.
//!
//! Palette from Geist Design System dark tokens (background `#000`,
//! gray scale, blue `#0070F3`, green/red/amber semantic colors).

use ratatui::style::{Color, Modifier};

use super::tokyonight::Theme;

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

#[allow(dead_code)]
mod palette {
    use super::*;

    pub const BG: Color = rgb(0, 0, 0); // #000000
    pub const BG_ALT: Color = rgb(10, 10, 10); // #0A0A0A
    pub const GRAY100: Color = rgb(26, 26, 26); // #1A1A1A
    pub const GRAY200: Color = rgb(31, 31, 31); // #1F1F1F
    pub const GRAY300: Color = rgb(41, 41, 41); // #292929
    pub const GRAY400: Color = rgb(46, 46, 46); // #2E2E2E
    pub const GRAY500: Color = rgb(69, 69, 69); // #454545
    pub const GRAY600: Color = rgb(135, 135, 135); // #878787
    pub const GRAY700: Color = rgb(143, 143, 143); // #8F8F8F
    pub const GRAY900: Color = rgb(161, 161, 161); // #A1A1A1
    pub const GRAY1000: Color = rgb(237, 237, 237); // #EDEDED

    pub const BLUE700: Color = rgb(0, 112, 243); // #0070F3
    pub const BLUE900: Color = rgb(82, 168, 255); // #52A8FF
    pub const RED700: Color = rgb(229, 72, 77); // #E5484D
    pub const RED900: Color = rgb(255, 97, 102); // #FF6166
    pub const AMBER700: Color = rgb(255, 178, 36); // #FFB224
    pub const GREEN700: Color = rgb(70, 167, 88); // #46A758
    pub const GREEN900: Color = rgb(99, 196, 109); // #63C46D
    pub const TEAL900: Color = rgb(10, 199, 172); // #0AC7AC
    pub const PURPLE700: Color = rgb(142, 78, 198); // #8E4EC6
    pub const PURPLE900: Color = rgb(191, 122, 240); // #BF7AF0
}
use palette::*;

impl Theme {
    pub const fn vercel() -> Self {
        Self {
            bg_base: BG,
            bg_light: GRAY300,
            bg_dark: GRAY100,
            bg_highlight: GRAY200,
            bg_hover: GRAY400,
            bg_terminal: BG,

            accent_user: GRAY1000,
            accent_assistant: BLUE900,
            accent_thinking: GRAY600,
            accent_tool: GRAY700,
            accent_system: BLUE700,
            accent_error: RED700,
            accent_success: GREEN700,
            accent_running: BLUE900,
            accent_skill: PURPLE700,

            text_primary: GRAY1000,
            text_secondary: GRAY900,

            gray_dim: GRAY500,
            gray: GRAY600,
            gray_bright: GRAY900,

            command: AMBER700,
            path: BLUE900,
            running: TEAL900,
            warning: AMBER700,

            fuzzy_accent: BLUE900,

            accent_plan: AMBER700,
            accent_verify: PURPLE900,
            accent_feedback: GREEN900,
            accent_remember: GREEN900,

            selection_border: GRAY500,
            hover_border: GRAY400,
            prompt_border: GRAY400,
            prompt_border_active: GRAY1000,

            accent_model: BLUE900,

            scrollbar_bg: GRAY100,
            scrollbar_fg: GRAY600,

            diff_delete_bg: rgb(42, 19, 20),
            diff_delete_fg: RED900,
            diff_insert_bg: rgb(11, 29, 15),
            diff_insert_fg: GREEN900,
            diff_equal_fg: GRAY600,
            diff_gutter_fg: GRAY600,

            bg_visual: GRAY300,

            paste_bg: GRAY100,
            paste_fg: GRAY900,
            paste_dim: GRAY600,

            md_heading_h1: GRAY1000,
            md_heading_h1_mod: Modifier::BOLD,
            md_heading_h2: PURPLE900,
            md_heading_h2_mod: Modifier::BOLD,
            md_heading_h3: BLUE900,
            md_heading_h3_mod: Modifier::BOLD,
            md_heading_h4: TEAL900,
            md_heading_h4_mod: Modifier::BOLD.union(Modifier::ITALIC),
            md_heading_h5: AMBER700,
            md_heading_h5_mod: Modifier::BOLD,
            md_heading_h6: GRAY900,
            md_heading_h6_mod: Modifier::BOLD,
            md_code: BLUE900,
            md_task_checked: GREEN900,
            md_task_unchecked: GRAY900,
            md_muted: GRAY600,
            md_code_bg: GRAY100,
            md_text: GRAY1000,
            link_fg: BLUE900,
        }
    }
}
