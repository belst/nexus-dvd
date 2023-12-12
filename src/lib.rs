use arcdps_imgui::{
    self,
    sys::{igSetAllocatorFunctions, igSetCurrentContext},
    Context, Image, Slider, Ui, Window,
};
use nexus_rs::raw_structs::{
    AddonAPI, AddonDefinition, AddonVersion, EAddonFlags, ELogLevel, ERenderType, NexusLinkData,
    Texture, LPVOID,
};
use once_cell::sync::Lazy;
use rand::{seq::SliceRandom, Rng};
use std::{
    ffi::{c_ulong, c_void, CStr},
    fs::{create_dir_all, File},
    io::{BufRead, BufReader, Write},
    mem::MaybeUninit,
    path::PathBuf,
    ptr::{self, NonNull},
    time::{Duration, Instant},
};
use windows::{
    core::s,
    Win32::{
        Foundation::{HINSTANCE, HMODULE},
        System::{LibraryLoader::DisableThreadLibraryCalls, SystemServices},
    },
};

static mut API: MaybeUninit<&'static AddonAPI> = MaybeUninit::uninit();
static mut CTX: MaybeUninit<Context> = MaybeUninit::uninit();
static mut UI: MaybeUninit<Ui> = MaybeUninit::uninit();
static mut NEXUS_DATA: Option<&'static NexusLinkData> = None;
static mut DVD_ICON: Option<&'static Texture> = None;
static mut DVD_ICON_FILE: Option<&'static Texture> = None;
static mut HANDLE: Option<HMODULE> = None;

#[no_mangle]
unsafe extern "C" fn DllMain(
    hinst_dll: HINSTANCE,
    fdw_reason: c_ulong,
    _lpv_reserveded: LPVOID,
) -> bool {
    if fdw_reason == SystemServices::DLL_PROCESS_ATTACH {
        let _ = DisableThreadLibraryCalls(hinst_dll);
        HANDLE = Some(hinst_dll.into());
    }
    true
}

unsafe extern "C" fn texture_callback_resource(id: *const i8, text: *mut Texture) {
    (API.assume_init().log)(
        ELogLevel::INFO,
        format!("{} Loaded\0", CStr::from_ptr(id).to_str().unwrap()).as_ptr() as _,
    );
    DVD_ICON = Some(&*text);
}
unsafe extern "C" fn texture_callback_file(id: *const i8, text: *mut Texture) {
    (API.assume_init().log)(
        ELogLevel::INFO,
        format!("{} Loaded\0", CStr::from_ptr(id).to_str().unwrap()).as_ptr() as _,
    );
    DVD_ICON_FILE = Some(&*text);
}

unsafe fn load_file() {
    if USE_FILE_IMAGE && DVD_ICON_FILE.is_none() {
        let api = API.assume_init();
        (api.load_texture_from_file)(
            s!("DVD_ICON_FILE").0 as _,
            (api.get_addon_directory)(s!("dvd\\dvd.png").0 as _),
            texture_callback_file,
        );
    }
}
unsafe extern "C" fn load(a_api: *mut AddonAPI) {
    let api = &*a_api;
    API.write(api);

    igSetCurrentContext(api.imgui_context);
    igSetAllocatorFunctions(
        Some(api.imgui_malloc),
        Some(api.imgui_free),
        ptr::null::<c_void>() as *mut _,
    );

    CTX.write(Context::current());
    UI.write(Ui::from_ctx(CTX.assume_init_ref()));
    let data = (api.get_resource)(s!("DL_NEXUS_LINK").0 as _) as *const NexusLinkData;
    if data.is_null() {
        (api.log)(
            ELogLevel::CRITICAL,
            s!("Could not find DL_NEXUS_LINK data.").0 as _,
        );
    } else {
        NEXUS_DATA = Some(&*data);
    }
    // static DVD_ICON_DATA: &'static [u8] = include_bytes!("dvd.png");
    // let p: *const i8 = (api.get_addon_directory)(b"/dvd.png\0" as *const _ as _);

    load_settings();
    load_file();

    if let Some(h) = HANDLE {
        (api.load_texture_from_resource)(
            s!("ICON_DVD").0 as _,
            101,
            h.0 as _,
            texture_callback_resource,
        );
    }
    // (api.load_texture_from_file)(b"ICON_DVD\0" as *const _ as _, p, texture_callback);
    // Add an options window and a regular render callback
    (api.register_render)(ERenderType::Render, render);
    (api.register_render)(ERenderType::OptionsRender, render_options);

    (api.log)(ELogLevel::INFO, s!("DVD addon was loaded.").0 as _);
}
unsafe extern "C" fn unload() {
    (API.assume_init().unregister_render)(render);
    (API.assume_init().unregister_render)(render_options);
    store_settings();
}

static mut SPEED_VAL: f32 = 2f32;
static mut DVD_COUNT: u32 = 1;
static mut USE_FILE_IMAGE: bool = false;
static mut SHOW_DURING_GAMEPLAY: bool = false;

unsafe fn config_path() -> PathBuf {
    let api = API.assume_init();
    let config_path = CStr::from_ptr((api.get_addon_directory)(s!("dvd\\dvd.conf").0 as _))
        .to_string_lossy()
        .into_owned();
    config_path.into()
}

unsafe fn load_settings() {
    let Ok(file) = File::open(config_path()) else {
        return;
    };
    let f = BufReader::new(file);
    let mut it = f.lines();
    if let Some(Ok(speed)) = it.next() {
        if let Ok(speed) = speed.parse() {
            SPEED_VAL = speed;
        }
    }
    if let Some(Ok(count)) = it.next() {
        if let Ok(count) = count.parse() {
            DVD_COUNT = count;
        }
    }
    if let Some(Ok(file_image)) = it.next() {
        if let Ok(file_image) = file_image.parse() {
            USE_FILE_IMAGE = file_image;
        }
    }
    if let Some(Ok(show_during_gameplay)) = it.next() {
        if let Ok(show_during_gameplay) = show_during_gameplay.parse() {
            SHOW_DURING_GAMEPLAY = show_during_gameplay;
        }
    }
}
unsafe fn store_settings() {
    let path = config_path();
    let prefix = path.parent().unwrap();
    create_dir_all(prefix).ok();
    let Ok(mut file) = File::create(config_path()) else {
        return;
    };
    let mut config = format!(
        "{}\n{}\n{}\n{}",
        SPEED_VAL, DVD_COUNT, USE_FILE_IMAGE, SHOW_DURING_GAMEPLAY
    );
    file.write_all(config.as_bytes_mut()).ok();
}

pub unsafe extern "C" fn render_options() {
    let ui = unsafe { UI.assume_init_ref() };

    ui.separator();
    Slider::new("DVD Speed", 1f32, 50f32).build(ui, &mut SPEED_VAL);
    Slider::new("DVD Count", 1u32, 50).build(ui, &mut DVD_COUNT);
    if ui.checkbox("Use image file (addons/dvd/dvd.png)", &mut USE_FILE_IMAGE) && USE_FILE_IMAGE {
        load_file();
    }
    ui.checkbox(
        "Show small version during gameplay",
        &mut SHOW_DURING_GAMEPLAY,
    );
}

#[derive(Debug)]
struct DvdState {
    x: f32,
    y: f32,
    direction: [i32; 2],
    tint: [f32; 4],
}

static mut STATE: Lazy<Vec<DvdState>> = Lazy::new(Vec::new);
unsafe fn calculate_pos() {
    static mut LAST_TS: Lazy<Instant> = Lazy::new(Instant::now);
    let elapsed = LAST_TS.elapsed().as_millis() as u64;
    let max_iterations = elapsed / 16;

    let mut iterations = max_iterations + 1;
    while 0 < iterations {
        if DVD_ICON.is_none() {
            return;
        }
        if NEXUS_DATA.is_none() {
            return;
        }
        if !SHOW_DURING_GAMEPLAY && NEXUS_DATA.unwrap().is_gameplay {
            return;
        }
        let delta = if iterations == 1 {
            Duration::from_millis(elapsed - 16 * max_iterations)
        } else {
            Duration::from_millis(16)
        };
        iterations -= 1;
        let nexus_data = NEXUS_DATA.unwrap();
        let dvd_icon = if USE_FILE_IMAGE {
            if let Some(icon) = DVD_ICON_FILE {
                icon
            } else {
                // placeholder
                DVD_ICON.unwrap()
            }
        } else {
            DVD_ICON.unwrap()
        };
        let icon_width = if nexus_data.is_gameplay && SHOW_DURING_GAMEPLAY {
            dvd_icon.width / 5
        } else {
            dvd_icon.width
        };
        let icon_height = if nexus_data.is_gameplay && SHOW_DURING_GAMEPLAY {
            dvd_icon.height / 5
        } else {
            dvd_icon.height
        };
        let count = DVD_COUNT as usize;
        while STATE.len() < count {
            STATE.push(DvdState {
                x: rand::thread_rng().gen_range(0..(nexus_data.width - icon_width)) as _,
                y: rand::thread_rng().gen_range(0..(nexus_data.height - icon_height)) as _,
                direction: *[[-1, -1], [-1, 1], [1, -1], [1, 1]]
                    .choose(&mut rand::thread_rng())
                    .unwrap(),
                tint: randomize_color(),
            })
        }
        STATE.truncate(count);
        let speed = SPEED_VAL;
        for state in STATE.iter_mut() {
            // dont just flip but keep direction consistent if they are outside for more than one frame
            if state.x < 0f32 {
                state.direction[0] = 1;
                state.tint = randomize_color();
            } else if state.x + icon_width as f32 > nexus_data.width as f32 {
                state.direction[0] = -1;
                state.tint = randomize_color();
            }
            if state.y < 0f32 {
                state.direction[1] = 1;
                state.tint = randomize_color();
            } else if state.y + icon_height as f32 > nexus_data.height as f32 {
                state.direction[1] = -1;
                state.tint = randomize_color();
            }
            state.x += speed * (delta.as_millis() as f32 / 16.) * state.direction[0] as f32;
            state.y += speed * (delta.as_millis() as f32 / 16.) * state.direction[1] as f32;
        }
    }
    *LAST_TS = Instant::now();
}

pub unsafe extern "C" fn render() {
    if let Some(nd) = NEXUS_DATA {
        if nd.is_gameplay && !SHOW_DURING_GAMEPLAY {
            return;
        }
    } else {
        return;
    }

    calculate_pos();
    let state = &STATE;

    for (i, dvd) in state.iter().enumerate() {
        render_dvd(i, dvd);
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

fn render_dvd(index: usize, dvd: &DvdState) {
    let ui = unsafe { UI.assume_init_ref() };
    let dvd_icon = if unsafe { USE_FILE_IMAGE && DVD_ICON_FILE.is_some() } {
        unsafe { DVD_ICON_FILE.unwrap() }
    } else {
        unsafe { DVD_ICON.unwrap() }
    };
    let nexus_data = unsafe { NEXUS_DATA.unwrap() };

    let show_during_gameplay = unsafe { SHOW_DURING_GAMEPLAY };
    let icon_width = if nexus_data.is_gameplay && show_during_gameplay {
        dvd_icon.width / 5
    } else {
        dvd_icon.width
    };
    let icon_height = if nexus_data.is_gameplay && show_during_gameplay {
        dvd_icon.height / 5
    } else {
        dvd_icon.height
    };
    if let Some(w) = Window::new(format!("DVD#{index}"))
        .no_decoration()
        .always_auto_resize(true)
        .draw_background(false)
        .movable(false)
        .no_inputs()
        .focus_on_appearing(false)
        .position([dvd.x, dvd.y], arcdps_imgui::Condition::Always)
        .begin(ui)
    {
        Image::new(
            (dvd_icon.resource).into(),
            [icon_width as _, icon_height as _],
        )
        .tint_col(dvd.tint)
        .build(ui);
        w.end();
    }
}

#[no_mangle]
pub extern "C" fn GetAddonDef() -> *mut AddonDefinition {
    static AD: AddonDefinition = AddonDefinition {
        signature: -69420,
        apiversion: nexus_rs::raw_structs::NEXUS_API_VERSION,
        name: s!("DVD").0 as _,
        version: AddonVersion {
            major: 0,
            minor: 8,
            build: 0,
            revision: 0,
        },
        author: s!("belst").0 as _,
        description: s!("Bouncy").0 as _,
        load,
        unload: Some(unsafe { NonNull::new_unchecked(unload as _) }),
        flags: EAddonFlags::None,
        provider: nexus_rs::raw_structs::EUpdateProvider::GitHub,
        update_link: Some(unsafe {
            NonNull::new_unchecked(s!("https://github.com/belst/nexus-dvd").0 as _)
        }),
    };

    &AD as *const _ as _
}
