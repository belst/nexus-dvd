use std::{
    fs::{create_dir_all, File},
    io::{BufRead, BufReader, Write},
    path::Path,
    sync::OnceLock,
};

use nexus::imgui::{Slider, Ui};

use crate::dvd;

#[derive(Debug, Default)]
pub(crate) struct Settings {
    pub(crate) use_file: bool,
    pub(crate) speed: f32,
    pub(crate) count: u32,
    pub(crate) show_during_gameplay: bool,
}

static mut SETTINGS: OnceLock<Settings> = OnceLock::new();

impl Settings {
    pub(crate) fn get() -> &'static Self {
        unsafe { SETTINGS.get_or_init(Default::default) }
    }

    pub(crate) fn get_mut() -> &'static mut Self {
        match unsafe { SETTINGS.get_mut() } {
            Some(s) => s,
            None => {
                Settings::get();
                Settings::get_mut()
            }
        }
    }

    pub(crate) fn load(path: impl AsRef<Path>) -> Result<(), Box<dyn std::error::Error>> {
        let settings = Self::get_mut();
        let file = File::open(path)?;
        let f = BufReader::new(file);
        let mut it = f.lines();
        if let Some(Ok(speed)) = it.next() {
            settings.speed = speed.parse()?;
        }
        if let Some(Ok(count)) = it.next() {
            settings.count = count.parse()?;
        }
        if let Some(Ok(file_image)) = it.next() {
            settings.use_file = file_image.parse()?;
        }
        if let Some(Ok(show_during_gameplay)) = it.next() {
            settings.show_during_gameplay = show_during_gameplay.parse()?;
        }

        Ok(())
    }
    pub(crate) fn store(path: impl AsRef<Path>) -> Result<(), Box<dyn std::error::Error>> {
        let settings = Settings::get();
        let prefix = path.as_ref().parent().unwrap();
        create_dir_all(prefix).ok();
        let mut file = File::create(path)?;
        write!(
            &mut file,
            "{}\n{}\n{}\n{}",
            settings.speed, settings.count, settings.use_file, settings.show_during_gameplay
        )?;

        Ok(())
    }

    pub(crate) fn render(ui: &Ui) {
        let settings = Settings::get_mut();
        Slider::new("DVD Speed", 1f32, 50f32).build(ui, &mut settings.speed);
        Slider::new("DVD Count", 1u32, 50).build(ui, &mut settings.count);
        if ui.checkbox(
            "Use image file (addons/dvd/dvd.png)",
            &mut settings.use_file,
        ) && settings.use_file
        {
            dvd::load_file();
        }
        ui.checkbox(
            "Show small version during gameplay",
            &mut settings.show_during_gameplay,
        );
    }
}
