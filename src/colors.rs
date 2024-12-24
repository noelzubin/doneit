use std::str::FromStr;

use ratatui::style::Color;


pub struct Theme {
    pub text: Color,
    pub text_dark: Color,
    pub text_completed: Color,
    pub item_highlight: Color,

    pub active_highlight: Color,
    pub inactive_highlight: Color,
    pub highlight_text_secondary: Color,
} 


impl Default for Theme {
    fn default() -> Self {
        Self {
            text: Color::from_str("#cad3f5").unwrap(),
            text_completed: Color::from_str("#494d64").unwrap(),
            text_dark: Color::from_str("#181926").unwrap(),
            highlight_text_secondary: Color::from_str("#24273a").unwrap(),

            active_highlight: Color::from_str("#b7bdf8").unwrap(),
            inactive_highlight: Color::from_str("#6e738d").unwrap(),
            item_highlight: Color::from_str("#6e738d").unwrap(),
        }
    }
}