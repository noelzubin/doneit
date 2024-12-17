use std::str::FromStr;

use ratatui::style::Color;


pub struct Theme {
    pub text: Color,
    pub block_highlight: Color,
    pub faded_text: Color,
    pub text_black: Color,
    pub highlight_text_secondary: Color,
    pub higlight_bg: Color,


    pub block_faded: Color,
} 


impl Theme {
    pub fn new() -> Self {
        Self {
            text: Color::from_str("#cad3f5").unwrap(),
            faded_text: Color::from_str("#494d64").unwrap(),
            text_black: Color::from_str("#181926").unwrap(),
            highlight_text_secondary: Color::from_str("#24273a").unwrap(),

            block_highlight: Color::from_str("#b7bdf8").unwrap(),
            block_faded: Color::from_str("#6e738d").unwrap(),
            higlight_bg: Color::from_str("#6e738d").unwrap(),
        }
    }
}