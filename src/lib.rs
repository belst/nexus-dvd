use arcdps_imgui::{
    self,
    sys::{igSetAllocatorFunctions, igSetCurrentContext},
    Context, Image, Slider, Ui, Window,
};
use atomic_float::AtomicF32;
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
    ops::Neg,
    path::PathBuf,
    ptr::{self, NonNull},
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        RwLock,
    },
    thread::JoinHandle,
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
static mut POS_THREAD: Option<JoinHandle<()>> = None;

#[no_mangle]
unsafe extern "C" fn DllMain(
    hinst_dll: HINSTANCE,
    fdw_reason: c_ulong,
    _lpv_reserveded: LPVOID,
) -> bool {
    match fdw_reason {
        SystemServices::DLL_PROCESS_ATTACH => {
            let _ = DisableThreadLibraryCalls(hinst_dll);
            HANDLE = Some(hinst_dll.into());
        }
        _ => {}
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

fn load_file() {
    if USE_FILE_IMAGE.load(Ordering::Acquire) && unsafe { DVD_ICON_FILE.is_none() } {
        unsafe {
            let api = API.assume_init();
            (api.load_texture_from_file)(
                s!("DVD_ICON_FILE").0 as _,
                (api.get_addon_directory)(s!("dvd\\dvd.png").0 as _),
                texture_callback_file,
            );
        }
    }
}
unsafe extern "C" fn load(a_api: *mut AddonAPI) {
    let api = &*a_api;
    API.write(&api);

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
    UNLOAD.store(false, Ordering::SeqCst);
    POS_THREAD = Some(std::thread::spawn(|| calculate_pos()));
}
unsafe extern "C" fn unload() {
    (API.assume_init().unregister_render)(render);
    (API.assume_init().unregister_render)(render_options);
    UNLOAD.store(true, Ordering::SeqCst);
    store_settings();
    if let Some(h) = POS_THREAD.take() {
        let _ = h.join();
    }
}

static SPEED_VAL: AtomicF32 = AtomicF32::new(2f32);
static DVD_COUNT: AtomicU32 = AtomicU32::new(1);
static USE_FILE_IMAGE: AtomicBool = AtomicBool::new(false);
static SHOW_DURING_GAMEPLAY: AtomicBool = AtomicBool::new(false);

unsafe fn config_path() -> PathBuf {
    let api = API.assume_init();
    let config_path = CStr::from_ptr((api.get_addon_directory)(s!("dvd\\dvd.conf").0 as _))
        .to_string_lossy()
        .into_owned();
    return config_path.into();
}

unsafe fn load_settings() {
    let Ok(file) = File::open(config_path()) else {
        return;
    };
    let f = BufReader::new(file);
    let mut it = f.lines();
    if let Some(Ok(speed)) = it.next() {
        if let Ok(speed) = speed.parse() {
            SPEED_VAL.store(speed, Ordering::Release);
        }
    }
    if let Some(Ok(count)) = it.next() {
        if let Ok(count) = count.parse() {
            DVD_COUNT.store(count, Ordering::Release);
        }
    }
    if let Some(Ok(file_image)) = it.next() {
        if let Ok(file_image) = file_image.parse() {
            USE_FILE_IMAGE.store(file_image, Ordering::Release);
        }
    }
    if let Some(Ok(show_during_gameplay)) = it.next() {
        if let Ok(show_during_gameplay) = show_during_gameplay.parse() {
            SHOW_DURING_GAMEPLAY.store(show_during_gameplay, Ordering::Release);
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
        SPEED_VAL.load(Ordering::Acquire),
        DVD_COUNT.load(Ordering::Acquire),
        USE_FILE_IMAGE.load(Ordering::Acquire),
        SHOW_DURING_GAMEPLAY.load(Ordering::Acquire)
    );
    file.write_all(config.as_bytes_mut()).ok();
}

pub extern "C" fn render_options() {
    let ui = unsafe { UI.assume_init_ref() };

    ui.separator();
    let _ = SPEED_VAL.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |mut speed| {
        Slider::new("DVD Speed", 1f32, 50f32).build(&ui, &mut speed);
        Some(speed)
    });
    let _ = DVD_COUNT.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |mut count| {
        Slider::new("DVD Count", 1u32, 50).build(&ui, &mut count);
        Some(count)
    });
    let _ = USE_FILE_IMAGE.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |mut use_image| {
        if ui.checkbox("Use image file (addons/dvd/dvd.png)", &mut use_image) {
            if use_image {
                load_file();
            }
        }
        Some(use_image)
    });
    let _ = SHOW_DURING_GAMEPLAY.fetch_update(
        Ordering::SeqCst,
        Ordering::SeqCst,
        |mut show_during_gameplay| {
            ui.checkbox(
                "Show small version during gameplay",
                &mut show_during_gameplay,
            );
            Some(show_during_gameplay)
        },
    );
}

#[derive(Debug)]
struct DvdState {
    x: f32,
    y: f32,
    direction: [i32; 2],
    tint: [f32; 4],
}

static mut UNLOAD: AtomicBool = AtomicBool::new(false);

static mut STATE: Lazy<RwLock<Vec<DvdState>>> = Lazy::new(|| RwLock::new(Vec::new()));
unsafe fn calculate_pos() {
    static mut LAST_TS: Lazy<Instant> = Lazy::new(|| Instant::now());
    loop {
        if UNLOAD.load(Ordering::SeqCst) {
            return;
        }
        std::thread::sleep(Duration::from_millis(16));
        if UNLOAD.load(Ordering::SeqCst) {
            return;
        }
        let delta = LAST_TS.elapsed();
        *LAST_TS = Instant::now();

        if DVD_ICON.is_none() {
            continue;
        }
        if NEXUS_DATA.is_none() {
            continue;
        }
        if !SHOW_DURING_GAMEPLAY.load(Ordering::Acquire) && NEXUS_DATA.unwrap().is_gameplay {
            continue;
        }
        let nexus_data = NEXUS_DATA.unwrap();
        let dvd_icon = if USE_FILE_IMAGE.load(Ordering::Acquire) {
            if let Some(icon) = DVD_ICON_FILE {
                icon
            } else {
                // placeholder
                DVD_ICON.unwrap()
            }
        } else {
            DVD_ICON.unwrap()
        };
        let icon_width = if nexus_data.is_gameplay && SHOW_DURING_GAMEPLAY.load(Ordering::Acquire) {
            dvd_icon.width / 5
        } else {
            dvd_icon.width
        };
        let icon_height = if nexus_data.is_gameplay && SHOW_DURING_GAMEPLAY.load(Ordering::Acquire)
        {
            dvd_icon.height / 5
        } else {
            dvd_icon.height
        };
        let count = DVD_COUNT.load(Ordering::Acquire) as usize;
        let mut state = STATE.write().expect("Poisend State");
        while state.len() < count {
            state.push(DvdState {
                x: rand::thread_rng().gen_range(0..(nexus_data.width - icon_width)) as _,
                y: rand::thread_rng().gen_range(0..(nexus_data.height - icon_height)) as _,
                direction: [[-1, -1], [-1, 1], [1, -1], [1, 1]]
                    .choose(&mut rand::thread_rng())
                    .unwrap()
                    .clone(),
                tint: randomize_color(),
            })
        }
        state.truncate(count);
        let speed = SPEED_VAL.load(Ordering::Acquire);
        for state in state.iter_mut() {
            let x_speed = colission(
                &mut state.x,
                (nexus_data.width - icon_width) as f32,
                delta,
                &mut state.direction[0],
                &mut state.tint,
                speed,
            );
            let y_speed = colission(
                &mut state.y,
                (nexus_data.height - icon_height) as f32,
                delta,
                &mut state.direction[1],
                &mut state.tint,
                speed,
            );
            let x_speed = colission(
                &mut state.x,
                (nexus_data.width - icon_width) as f32,
                delta,
                &mut state.direction[0],
                &mut state.tint,
                x_speed,
            );
            let y_speed = colission(
                &mut state.y,
                (nexus_data.height - icon_height) as f32,
                delta,
                &mut state.direction[1],
                &mut state.tint,
                y_speed,
            );

            state.x += x_speed * (delta.as_millis() as f32 / 16.) * state.direction[0] as f32;
            state.y += y_speed * (delta.as_millis() as f32 / 16.) * state.direction[1] as f32;
        }
    }
}

pub extern "C" fn render() {
    if let Some(nd) = unsafe { NEXUS_DATA } {
        if nd.is_gameplay && !SHOW_DURING_GAMEPLAY.load(Ordering::Acquire) {
            return;
        }
    } else {
        return;
    }
    let state = unsafe { &STATE }.read().expect("Poisend State");
    for (i, dvd) in state.iter().enumerate() {
        render_dvd(i as usize, dvd);
    }
}

fn randomize_color() -> [f32; 4] {
    let mut rng = rand::thread_rng();
    let mut color = [1.0; 4];
    color[0] = rng.gen_range(0. ..=1.);
    color[1] = rng.gen_range(0. ..=1.);
    color[2] = rng.gen_range(0. ..=1.);
    return color;
}

fn colission(
    axis: &mut f32,
    max_bound: f32,
    delta: Duration,
    direction: &mut i32,
    tint: &mut [f32; 4],
    speed: f32,
) -> f32 {
    if *axis < 0.0 || *axis >= max_bound {
        let c = axis.clamp(0.0, max_bound);
        let d = (*axis - c).abs();
        *axis = c;
        *direction = direction.neg();
        *axis += d * (delta.as_millis() as f32 / 16.0) * *direction as f32;
        *tint = randomize_color();
        return (speed - d).abs();
    }
    return speed;
}

fn render_dvd(index: usize, dvd: &DvdState) {
    let ui = unsafe { UI.assume_init_ref() };
    let dvd_icon = if USE_FILE_IMAGE.load(Ordering::Acquire) && unsafe { DVD_ICON_FILE.is_some() } {
        unsafe { DVD_ICON_FILE.unwrap() }
    } else {
        unsafe { DVD_ICON.unwrap() }
    };
    let nexus_data = unsafe { NEXUS_DATA.unwrap() };

    let show_during_gameplay = SHOW_DURING_GAMEPLAY.load(Ordering::Acquire);
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
            minor: 7,
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
