use crate::colors::Theme;
use directories::ProjectDirs;
use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;

#[derive(Serialize, Deserialize)]
pub struct ThemeConfig {
    pub text: String,
    pub text_dark: String,
    pub text_completed: String,
    pub item_highlight: String,

    pub active_highlight: String,
    pub inactive_highlight: String,
    pub highlight_text_secondary: String,
}

impl Into<Theme> for ThemeConfig {
    fn into(self) -> Theme {
        Theme {
            text: self.text.parse().unwrap(),
            text_dark: self.text_dark.parse().unwrap(),
            text_completed: self.text_completed.parse().unwrap(),
            item_highlight: self.item_highlight.parse().unwrap(),

            active_highlight: self.active_highlight.parse().unwrap(),
            inactive_highlight: self.inactive_highlight.parse().unwrap(),
            highlight_text_secondary: self.highlight_text_secondary.parse().unwrap(),
        }
    }
}

fn get_project_dirs() -> ProjectDirs {
    ProjectDirs::from("", "", "doneit".into())
        .expect("Failed to get project directories".into())
}

pub fn get_data_file_path() -> Result<PathBuf, std::io::Error> {
    let proj_dirs = get_project_dirs();
    let data_dir = proj_dirs.data_dir();
    if !data_dir.exists() {
        fs::create_dir_all(data_dir)?;
    }

    Ok(data_dir.join("doneit.json"))
}

pub fn get_theme() -> Theme {
    let proj_dirs = get_project_dirs();
    let config_dir = proj_dirs.config_dir();
    let theme_file_path = config_dir.join("theme.yaml");

    if theme_file_path.exists() {
        let mut file = File::open(theme_file_path).expect("Failed to open theme file");
        let mut contents = String::new();
        file.read_to_string(&mut contents).expect("Failed to read theme file");
        let theme_config: ThemeConfig = serde_yaml::from_str(&contents).expect("Failed to parse theme file");
        theme_config.into()
    } else {
        Theme::default()
    }
}
