use rand::prelude::*;
use std::{ptr::addr_of_mut, time::Instant};

use nexus::{
    data_link::{read_nexus_link, NexusLink},
    imgui::{Condition, Image, Ui, Window},
    paths::get_addon_dir,
    texture::{
        get_texture as nexus_get_texture, load_texture_from_file, load_texture_from_memory, Texture,
    },
};
use once_cell::sync::Lazy;
use rand::Rng;

use crate::settings::Settings;

static DVD_BYTES: &'static [u8] = include_bytes!("../dvd.png");
pub(crate) fn load() {
    load_texture_from_memory("DVD_ICON", DVD_BYTES, None);
}

pub(crate) fn load_file() {
    let settings = Settings::get();
    if settings.use_file && nexus_get_texture("DVD_ICON_FILE").is_none() {
        let addon_dir = get_addon_dir("dvd").expect("Invalid addon dir");
        load_texture_from_file("DVD_ICON_FILE", addon_dir.join("dvd.png"), None);
    }
}

pub(crate) fn get_texture() -> Option<Texture> {
    let settings = Settings::get();
    if settings.use_file {
        if let Some(text) = nexus_get_texture("DVD_ICON_FILE") {
            return Some(text);
        };
    }
    nexus_get_texture("DVD_ICON")
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
    let Some(text) = get_texture() else {
        return;
    };
    let ndata = read_nexus_link().expect("nexuslinkdata should exist");
    let settings = Settings::get();

    if !settings.show_during_gameplay && ndata.is_gameplay {
        return;
    }

    let state = unsafe { &mut *addr_of_mut!(STATE) };

    while state.len() < settings.count as _ {
        state.push(DvdState::rand(&text));
    }
    state.truncate(settings.count as _);

    static mut LAST_TS: Lazy<Instant> = Lazy::new(Instant::now);
    let elapsed = unsafe { LAST_TS.elapsed().as_millis() as u64 };
    for (i, d) in state.iter_mut().enumerate() {
        d.simulate(&text, elapsed);
        d.render(ui, &text, i);
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
    fn rand(texture: &Texture) -> Self {
        let mut rng = rand::thread_rng();
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

    fn collide(&mut self, texture: &Texture) -> bool {
        let mut collided = false;
        let ndata = read_nexus_link().expect("nexus link data should exist");
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

    fn simulate(&mut self, texture: &Texture, elapsed: u64) {
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
            collission = self.collide(texture);
            self.step(Settings::get().speed * (delta / 16.0));
        }

        if collission {
            self.tint = randomize_color();
        }
    }

    fn render(&self, ui: &Ui, texture: &Texture, index: usize) {
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
