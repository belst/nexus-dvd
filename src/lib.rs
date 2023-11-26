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
    ffi::{c_char, c_ulong, c_void, CStr},
    fs::{create_dir_all, File},
    io::{BufRead, BufReader, Write},
    mem::MaybeUninit,
    ops::Neg,
    path::PathBuf,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
    thread::JoinHandle,
    time::{Duration, Instant},
};
use windows::{
    core::s,
    Win32::{
        Foundation::{HINSTANCE, HMODULE},
        System::SystemServices,
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

unsafe fn load_file() {
    if USE_FILE_IMAGE && DVD_ICON_FILE.is_none() {
        let api = API.assume_init();
        (api.load_texture_from_file)(
            s!("DVD_ICON_FILE").0 as _,
            (api.get_addon_directory)(s!("/dvd/dvd.png").0 as _),
            texture_callback_file,
        );
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
    UNLOAD.store(true, Ordering::SeqCst);
    store_settings();
    if let Some(h) = POS_THREAD.take() {
        let _ = h.join();
    }
    (API.assume_init().unregister_render)(render);
    (API.assume_init().unregister_render)(render_options);
}

static mut SPEED_VAL: f32 = 2f32;
static mut DVD_COUNT: u32 = 1;
static mut USE_FILE_IMAGE: bool = false;

unsafe fn config_path() -> PathBuf {
    let api = API.assume_init();
    let config_path = CStr::from_ptr((api.get_addon_directory)(s!("/dvd/dvd.conf").0 as _))
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
        SPEED_VAL = speed.parse().unwrap_or(SPEED_VAL);
    }
    if let Some(Ok(count)) = it.next() {
        DVD_COUNT = count.parse().unwrap_or(DVD_COUNT);
    }
    if let Some(Ok(file_image)) = it.next() {
        USE_FILE_IMAGE = file_image.parse().unwrap_or(USE_FILE_IMAGE);
    }
}
unsafe fn store_settings() {
    let path = config_path();
    let prefix = path.parent().unwrap();
    create_dir_all(prefix).ok();
    let Ok(mut file) = File::create(config_path()) else {
        return;
    };
    let mut config = format!("{SPEED_VAL}\n{DVD_COUNT}\n{USE_FILE_IMAGE}");
    file.write_all(config.as_bytes_mut()).ok();
}

pub unsafe extern "C" fn render_options() {
    let ui = UI.assume_init_ref();

    ui.separator();
    Slider::new("DVD Speed", 1f32, 50f32).build(&ui, &mut SPEED_VAL);
    Slider::new("DVD Count", 1u32, 50).build(&ui, &mut DVD_COUNT);
    if ui.checkbox("Use image file (addons/dvd/dvd.png)", &mut USE_FILE_IMAGE) {
        if USE_FILE_IMAGE {
            load_file();
        }
    }
}

#[derive(Debug)]
struct DvdState {
    x: f32,
    y: f32,
    direction: [i32; 2],
    tint: [f32; 4],
}

static mut UNLOAD: AtomicBool = AtomicBool::new(false);

static mut STATE: Lazy<Vec<DvdState>> = Lazy::new(|| Vec::new());
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
        if !USE_FILE_IMAGE && DVD_ICON.is_none() {
            continue;
        }
        if USE_FILE_IMAGE && DVD_ICON_FILE.is_none() {
            continue;
        }
        if NEXUS_DATA.is_none() {
            continue;
        }
        if NEXUS_DATA.unwrap().is_gameplay {
            continue;
        }
        while STATE.len() < DVD_COUNT as usize {
            STATE.push(DvdState {
                x: rand::thread_rng()
                    .gen_range(0..(NEXUS_DATA.unwrap().width - DVD_ICON.unwrap().width))
                    as _,
                y: rand::thread_rng()
                    .gen_range(0..(NEXUS_DATA.unwrap().height - DVD_ICON.unwrap().height))
                    as _,
                direction: [[-1, -1], [-1, 1], [1, -1], [1, 1]]
                    .choose(&mut rand::thread_rng())
                    .unwrap()
                    .clone(),
                tint: randomize_color(),
            })
        }
        STATE.truncate(DVD_COUNT as usize);

        let nexus_data = NEXUS_DATA.unwrap();
        let dvd_icon = if USE_FILE_IMAGE {
            DVD_ICON_FILE.unwrap()
        } else {
            DVD_ICON.unwrap()
        };

        for state in STATE.iter_mut() {
            let x_speed = colission(
                &mut state.x,
                (nexus_data.width - dvd_icon.width) as f32,
                delta,
                &mut state.direction[0],
                &mut state.tint,
                unsafe { SPEED_VAL },
            );
            let y_speed = colission(
                &mut state.y,
                (nexus_data.height - dvd_icon.height) as f32,
                delta,
                &mut state.direction[1],
                &mut state.tint,
                unsafe { SPEED_VAL },
            );
            let x_speed = colission(
                &mut state.x,
                (nexus_data.width - dvd_icon.width) as f32,
                delta,
                &mut state.direction[0],
                &mut state.tint,
                x_speed,
            );
            let y_speed = colission(
                &mut state.y,
                (nexus_data.height - dvd_icon.height) as f32,
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

pub unsafe extern "C" fn render() {
    if let Some(nd) = NEXUS_DATA {
        if nd.is_gameplay {
            return;
        }
    }
    for (i, dvd) in STATE.iter().enumerate() {
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
    let dvd_icon = unsafe {
        if USE_FILE_IMAGE {
            DVD_ICON_FILE.unwrap()
        } else {
            DVD_ICON.unwrap()
        }
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
            [dvd_icon.width as _, dvd_icon.height as _],
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
        name: b"DVD\0".as_ptr() as *const c_char,
        version: AddonVersion {
            major: 0,
            minor: 4,
            build: 0,
            revision: 0,
        },
        author: b"belst\0".as_ptr() as *const c_char,
        description: b"Bouncy\0".as_ptr() as *const c_char,
        load,
        unload: Some(unload),
        flags: EAddonFlags::None,
        provider: nexus_rs::raw_structs::EUpdateProvider::GitHub,
        update_link: Some(s!("https://github.com/belst/nexus-dvd").0 as _),
    };

    &AD as *const _ as _
}
