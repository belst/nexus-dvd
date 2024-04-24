use rand::prelude::*;
use std::{sync::OnceLock, time::Instant};

use nexus::{
    data_link::{read_nexus_link, NexusLink},
    imgui::{Condition, Image, Ui, Window},
    paths::get_addon_dir,
    texture::{get_texture_or_create_from_file, get_texture_or_create_from_resource, Texture},
};
use once_cell::sync::Lazy;
use rand::Rng;

use crate::{settings::Settings, HANDLE};

static DVD_ICON: OnceLock<Texture> = OnceLock::new();
static DVD_ICON_FILE: OnceLock<Texture> = OnceLock::new();

pub(crate) fn load_from_resource() {
    if let Some(h) = unsafe { HANDLE } {
        if let Some(texture) = get_texture_or_create_from_resource("DVD_ICON", 101, h) {
            let _ = DVD_ICON.set(texture);
        } else {
            log::error!("Could not get texture from resource");
        }
    }
}

pub(crate) fn load_file() {
    let settings = Settings::get();
    if settings.use_file && DVD_ICON_FILE.get().is_none() {
        let addon_dir = get_addon_dir("dvd").expect("Invalid addon dir");
        if let Some(texture) =
            get_texture_or_create_from_file("DVD_ICON_FILE", addon_dir.join("dvd.png"))
        {
            let _ = DVD_ICON_FILE.set(texture);
        } else {
            log::warn!("Could not load dvd.png, check if it exists");
        }
    }
}

pub(crate) fn get_texture() -> &'static Texture {
    let settings = Settings::get();
    if settings.use_file {
        if let Some(texture) = DVD_ICON_FILE.get() {
            return texture;
        }
    }
    DVD_ICON.get().expect("DVD_ICON to exist")
}

static mut STATE: Lazy<Vec<DvdState>> = Lazy::new(Vec::new);
#[derive(Debug, Default)]
struct DvdState {
    x: f32,
    y: f32,
    direction: [f32; 2],
    tint: [f32; 4],
}

pub(crate) fn render_all(ui: &Ui) {
    if DVD_ICON.get().is_none() {
        return;
    }
    let ndata = read_nexus_link().expect("nexuslinkdata should exist");
    let settings = Settings::get();

    if !settings.show_during_gameplay && ndata.is_gameplay {
        return;
    }

    let state = unsafe { &mut STATE };

    while state.len() < settings.count as _ {
        state.push(DvdState::rand());
    }
    state.truncate(settings.count as _);

    static mut LAST_TS: Lazy<Instant> = Lazy::new(Instant::now);
    let elapsed = unsafe { LAST_TS.elapsed().as_millis() as u64 };
    for (i, d) in state.iter_mut().enumerate() {
        d.simulate(elapsed);
        d.render(ui, i);
    }
    unsafe {
        *LAST_TS = Instant::now();
    }
}

fn randomize_color() -> [f32; 4] {
    let mut rng = rand::thread_rng();
    let mut color = [1.0; 4];
    color[0] = rng.gen_range(0. ..=1.);
    color[1] = rng.gen_range(0. ..=1.);
    color[2] = rng.gen_range(0. ..=1.);
    color
}

fn random_direction() -> [f32; 2] {
    *[[-1.0, -1.0], [-1.0, 1.0], [1.0, -1.0], [1.0, 1.0]]
        .choose(&mut rand::thread_rng())
        .unwrap()
}

fn get_size(texture: &Texture, settings: &Settings, ndata: &NexusLink) -> [f32; 2] {
    if settings.show_during_gameplay && ndata.is_gameplay {
        texture.size_resized(0.2)
    } else {
        texture.size()
    }
}

impl DvdState {
    fn rand() -> Self {
        let mut rng = rand::thread_rng();
        let texture = get_texture();
        let settings = Settings::get();
        let ndata = read_nexus_link().expect("nexuslinkdata should exist");
        let [width, height] = get_size(texture, settings, &ndata);
        Self {
            direction: random_direction(),
            tint: randomize_color(),
            x: rng.gen_range(0.0..ndata.width as f32 - width),
            y: rng.gen_range(0.0..ndata.height as f32 - height),
        }
    }

    fn step(&mut self, speed: f32) {
        self.x += speed * self.direction[0];
        self.y += speed * self.direction[1];
    }

    fn collide(&mut self) -> bool {
        let mut collided = false;
        let ndata = read_nexus_link().expect("nexus link data should exist");
        let texture = get_texture();
        let [width, height] = get_size(texture, Settings::get(), &ndata);
        if self.x < 0.0 {
            self.direction[0] = 1.0;
            collided = true;
        }
        if self.x + width > ndata.width as _ {
            self.direction[0] = -1.0;
            collided = true;
        }
        if self.y < 0.0 {
            self.direction[1] = 1.0;
            collided = true;
        }
        if self.y + height > ndata.height as _ {
            self.direction[1] = -1.0;
            collided = true;
        }

        collided
    }

    fn simulate(&mut self, elapsed: u64) {
        let max_iterations = elapsed / 16;
        let mut iterations = max_iterations + 1;
        let mut collission = false;

        while 0 < iterations {
            let delta = if iterations == 1 {
                (elapsed - 16 * max_iterations) as f32
            } else {
                16.0
            };
            iterations -= 1;
            collission = self.collide();
            self.step(Settings::get().speed * (delta / 16.0));
        }

        if collission {
            self.tint = randomize_color();
        }
    }

    fn render(&self, ui: &Ui, index: usize) {
        let texture = get_texture();
        let ndata = read_nexus_link().expect("NexusLinkData");
        let size = get_size(texture, Settings::get(), &ndata);
        if let Some(_w) = Window::new(format!("DVD#{index}"))
            .no_decoration()
            .always_auto_resize(true)
            .draw_background(false)
            .movable(false)
            .no_inputs()
            .focus_on_appearing(false)
            .position([self.x, self.y], Condition::Always)
            .begin(ui)
        {
            Image::new(texture.id(), size).tint_col(self.tint).build(ui);
        }
    }
}
