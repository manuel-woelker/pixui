//! Default painter colors; applications can supply different painters entirely.
use crate::ui::{display_list::Color, presentation::Theme};

pub struct Palette {
    pub background: Color,
    pub foreground: Color,
    pub control: Color,
    pub accent: Color,
}
impl Palette {
    pub fn for_theme(theme: Theme) -> Self {
        match theme {
            Theme::Light => Self {
                background: Color(250, 250, 250),
                foreground: Color(25, 25, 25),
                control: Color(225, 230, 238),
                accent: Color(35, 95, 200),
            },
            Theme::Dark => Self {
                background: Color(25, 28, 34),
                foreground: Color(235, 235, 240),
                control: Color(55, 60, 70),
                accent: Color(130, 180, 255),
            },
        }
    }
}
