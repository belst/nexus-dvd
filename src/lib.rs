use nexus::{
    gui::{register_render, RenderType},
    imgui::Ui,
    paths::get_addon_dir,
    render, UpdateProvider,
};
use settings::Settings;
use std::path::PathBuf;

mod dvd;
mod settings;

fn load() {
    if let Err(e) = Settings::load(config_path()) {
        log::warn!("Could not load settings: {e}");
    }
    dvd::load_file();
    dvd::load();
    register_render(RenderType::Render, render!(render_fn)).revert_on_unload();
    register_render(RenderType::OptionsRender, render!(render_options)).revert_on_unload();

    log::info!("DVD addon was loaded");
}
fn unload() {
    if let Err(e) = Settings::store(config_path()) {
        log::error!("Could not store settings: {e}");
    }
}

fn config_path() -> PathBuf {
    get_addon_dir("dvd").expect("addon dir").join("dvd.conf")
}

fn render_options(ui: &Ui) {
    Settings::render(ui);
}

fn render_fn(ui: &Ui) {
    dvd::render_all(ui);
}

nexus::export! {
    signature: -69420,
    name: "DVD",
    load,
    unload,
    provider: UpdateProvider::GitHub,
    update_link: "https://github.com/belst/nexus-dvd"
}
