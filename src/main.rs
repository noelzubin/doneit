pub use app::App;
use store::Store;

pub mod app;
mod store;
mod colors;
mod config;



fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let terminal = ratatui::init();
    let data_path =  config::get_data_file_path()?;
    let store = Store::from_json_file(&data_path).unwrap_or_default();
    let mut app = App::new(store);
    let result = app.run(terminal);
    ratatui::restore();
    app.get_store().to_json_file(&data_path);
    result
}
