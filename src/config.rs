use std::fs;
use std::path::PathBuf;
use directories::ProjectDirs;


pub fn get_data_file_path() -> Result<PathBuf, std::io::Error> {
    let proj_dirs = ProjectDirs::from("", "", "doneit")
        .expect("Failed to get project directories");

    let data_dir = proj_dirs.data_dir();
    if !data_dir.exists() {
        fs::create_dir_all(data_dir)?;
    }

    Ok(data_dir.join("doneit.json"))
}
