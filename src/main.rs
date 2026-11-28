#![windows_subsystem = "windows"]
mod cadence;
mod discovery;
mod pairing;
mod quality;
mod super_motion;

const BACKGROUND: u32 = 0x101113;
const PANEL: u32 = 0x202124;
const TEXT: u32 = 0xf0efec;
const MUTED: u32 = 0xb4b3b0;
const BUTTON: u32 = 0x2b2c30;
const BUTTON_HOVER: u32 = 0x35363a;
const ACCENT: u32 = 0xe8e7e3;
const ACCENT_HOVER: u32 = 0xffffff;
const BORDER: u32 = 0x36373b;
const FOCUS: u32 = 0xe1e1dd;
const QUICK_WIDTH: i32 = 320;
const EDGE: i32 = 12;
const SIDEBAR: i32 = 224;
const BITRATES: [u32; 9] = [1, 5, 10, 15, 20, 30, 50, 80, 100];
const DEFAULT_BITRATE: u32 = 15;
use gpui::{prelude::*, *};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{ffi::c_void, os::windows::ffi::OsStrExt, path::PathBuf, time::Duration};
unsafe extern "C" {
    fn portal_login(media: *mut c_void, pin: *const std::ffi::c_char);
    fn portal_new(parent: isize, log: *const u16) -> *mut c_void;
    fn portal_free(media: *mut c_void);
    fn portal_open(media: *mut c_void, file: *const u16, wake: bool, bitrate_kbps: u32) -> i32;
    fn portal_preview(media: *mut c_void, file: *const u16) -> bool;
    fn portal_capture_window(media: *mut c_void) -> isize;
    fn portal_stop(media: *mut c_void);
    fn portal_motion_pid(pid: u32);
    fn portal_cancel_measure();
    fn portal_attach_panel(media: *mut c_void, panel: isize);
    fn portal_panel(media: *mut c_void, open: bool, width: i32, height: i32);
    fn portal_event(event: *const std::ffi::c_char);
    fn portal_pointer_x(media: *mut c_void) -> i32;
    fn portal_tick(media: *mut c_void, width: i32, show: bool) -> i32;
    fn portal_quality(media: *mut c_void, values: *const f32, count: usize) -> bool;
}
fn session_event(message: &str) {
    if let Ok(text) = std::ffi::CString::new(message) {
        unsafe { portal_event(text.as_ptr()) };
    }
}
struct Media(*mut c_void);
impl Drop for Media {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { portal_free(self.0) }
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
enum Page {
    Login,
    Stream,
    Network,
    About,
    Session,
    Pair,
    Quality,
    Dlss,
    FrameGen,
    Group(&'static str),
}
#[derive(Clone, Copy)]
enum Action {
    CaptureCadence,
    OpenReport,
    CloseQuick,
    PinQuick,
    Fullscreen,
    Login,
    Scan,
    SelectConsole(usize),
    Page(Page),
    Edit(usize),
    AccountHelp,
    ToggleFg,
    SettingMenu(usize),
    ChooseSetting(usize, f32),
    ManualPair,
    Pair,
    Connect,
    Stop,
    Wake,
    Change(usize, f32),
    ToggleBitrateMenu,
    Bitrate(u32),
}
struct Portal {
    logo: std::sync::Arc<Image>,
    motion: super_motion::Motion,
    media: Media,
    profile: PathBuf,
    quality_file: PathBuf,
    bitrate_file: PathBuf,
    bitrate: u32,
    main_window: WindowHandle<Portal>,
    fullscreen: bool,
    feedback: Option<(usize, std::time::Instant)>,
    feedback_text: String,
    capturing: bool,
    report: Option<PathBuf>,
    capture_message: String,
    quick_open: bool,
    pinned: bool,
    leave_at: Option<std::time::Instant>,
    previous_x: i32,
    previous_multiplier: f32,
    bitrate_menu_open: bool,
    setting_menu: Option<usize>,
    fields: Vec<quality::Field>,
    values: Vec<f32>,
    page: Page,
    inputs: [String; 3],
    selected: usize,
    busy: bool,
    status: i32,
    effect_status: i32,
    notice: String,
    focus: FocusHandle,
    discovery: discovery::State,
}
impl Page {
    fn is_dialog(self) -> bool {
        matches!(self, Self::Pair | Self::Login)
    }
}

impl Portal {
    fn new(window: &Window, cx: &mut Context<Self>) -> Self {
        let handle = HasWindowHandle::window_handle(window).unwrap();
        let RawWindowHandle::Win32(hwnd) = handle.as_raw() else {
            unreachable!()
        };
        let preview = std::env::args_os().skip(1).collect::<Vec<_>>();
        let preview = match preview.as_slice() {
            [flag, path] if flag == "--test-video" => Some(PathBuf::from(path)),
            _ => None,
        };
        let root = PathBuf::from(std::env::var_os("LOCALAPPDATA").expect("LOCALAPPDATA")).join(
            if preview.is_some() {
                "VeloraNR-test-video"
            } else {
                "PortalPS5"
            },
        );
        let mut notice = std::fs::create_dir_all(&root)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        let profile = root.join("console.dat");
        let quality_file = root.join("quality.json");
        let bitrate_file = root.join("bitrate.txt");
        let bitrate = std::fs::read_to_string(&bitrate_file)
            .ok()
            .and_then(|value| value.trim().parse().ok())
            .filter(|value| BITRATES.contains(value))
            .unwrap_or(DEFAULT_BITRATE);
        let fields = quality::schema();
        let mut values: Vec<_> = fields.iter().map(|f| f.initial).collect();
        let media = Media(unsafe {
            portal_new(
                hwnd.hwnd.get(),
                root.join("stream.log")
                    .as_os_str()
                    .encode_wide()
                    .chain(Some(0))
                    .collect::<Vec<_>>()
                    .as_ptr(),
            )
        });
        if media.0.is_null() {
            notice = "Superfície de vídeo indisponível.".into()
        } else {
            if let Ok(bytes) = std::fs::read(&quality_file) {
                if let Ok(saved) =
                    serde_json::from_slice::<std::collections::BTreeMap<String, f32>>(&bytes)
                {
                    for (value, field) in values.iter_mut().zip(&fields) {
                        if let Some(v) = saved.get(field.path) {
                            *value = *v
                        }
                    }
                }
            }
            quality::normalize(&fields, &mut values);
            if !unsafe { portal_quality(media.0, values.as_ptr(), values.len()) } {
                values = fields.iter().map(|f| f.initial).collect();
                if !unsafe { portal_quality(media.0, values.as_ptr(), values.len()) } {
                    notice = "Configuração inicial recusada.".into();
                }
            }
        }
        if let Some(path) = &preview {
            let file: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            if media.0.is_null()
                || !path.is_file()
                || !unsafe { portal_preview(media.0, file.as_ptr()) }
            {
                notice = "Não foi possível abrir o vídeo de teste.".into();
            }
        }
        let backend_index = fields
            .iter()
            .position(|f| f.path == "frameGenerationBackend")
            .unwrap();
        let fps_index = fields
            .iter()
            .position(|f| f.path == "finalFpsOnly")
            .unwrap();
        let multiplier_index = fields.iter().position(|f| f.path == "multiplier").unwrap();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(12))
                    .await;
                if this
                    .update(cx, |p, cx| {
                        if !p.media.0.is_null() {
                            let playing = p.page == Page::Stream && matches!(p.status, 2..=3);
                            if playing {
                                let x = unsafe { portal_pointer_x(p.media.0) };
                                let open = if p.pinned || p.setting_menu.is_some() {
                                    p.leave_at = None;
                                    true
                                } else if p.quick_open && !(0..QUICK_WIDTH + 10).contains(&x) {
                                    p.leave_at
                                        .get_or_insert_with(std::time::Instant::now)
                                        .elapsed()
                                        < std::time::Duration::from_millis(650)
                                } else {
                                    p.leave_at = None;
                                    p.quick_open
                                        || ((0..EDGE).contains(&x)
                                            && !(0..EDGE).contains(&p.previous_x))
                                };
                                p.previous_x = x;
                                if open != p.quick_open {
                                    p.quick_open = open;
                                    cx.notify();
                                }
                            } else {
                                p.quick_open = false;
                                p.pinned = false;
                                p.leave_at = None;
                                p.previous_x = -1;
                            }
                            unsafe {
                                portal_panel(
                                    p.media.0,
                                    playing
                                        && (p.quick_open
                                            || !p.feedback_text.is_empty()
                                            || p.motion.final_fps.is_some()),
                                    if !p.quick_open && p.feedback_text.is_empty() {
                                        100
                                    } else {
                                        QUICK_WIDTH
                                    },
                                    if !p.quick_open { 48 } else { 0 },
                                )
                            };
                            let sidebar = if playing && p.pinned {
                                QUICK_WIDTH
                            } else if playing {
                                0
                            } else {
                                SIDEBAR
                            };
                            let state = unsafe {
                                portal_tick(
                                    p.media.0,
                                    sidebar,
                                    p.page == Page::Stream && matches!(p.status, 2..=3),
                                )
                            };
                            let effects = state >> 8;
                            let state = state & 0xff;
                            let wanted = if p.page == Page::Stream
                                && state == 2
                                && p.status == 2
                                && p.values[backend_index] as u32 >= super_motion::FIRST_EXTERNAL
                                && effects & 15 <= 1
                                && p.values[multiplier_index] >= 2.
                            {
                                p.values[multiplier_index] as u32
                            } else {
                                0
                            };
                            let hwnd = if wanted != 0 {
                                unsafe { portal_capture_window(p.media.0) }
                            } else {
                                0
                            };
                            if p.motion.tick(
                                &root,
                                hwnd,
                                if hwnd == 0 { 0 } else { wanted },
                                p.values[backend_index] as u32,
                                p.values[fps_index] > 0.,
                            ) {
                                cx.notify();
                            }
                            if effects != p.effect_status {
                                p.effect_status = effects;
                                cx.notify();
                            }
                            if let Some((i, started)) = p.feedback {
                                let message = p.applied_feedback(i, started.elapsed());
                                if p.feedback_text != message {
                                    session_event(&format!(
                                        "effect={} status={message}",
                                        p.fields[i].path
                                    ));
                                    p.feedback_text = message;
                                    cx.notify();
                                }
                                if !playing || started.elapsed() >= Duration::from_secs(8) {
                                    cx.notify();
                                    p.feedback = None;
                                    p.feedback_text.clear();
                                }
                            }
                            if state != p.status {
                                p.status = state;
                                if state == 7 {
                                    p.page = Page::Login;
                                    p.selected = 2;
                                    p.inputs[2].clear();
                                } else if p.page == Page::Login {
                                    p.page = Page::Stream;
                                }
                                cx.notify();
                            }
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        let mut portal = Self {
            logo: std::sync::Arc::new(Image::from_bytes(
                ImageFormat::Png,
                include_bytes!("../assets/logo/veloranr-logo.png").to_vec(),
            )),
            motion: Default::default(),
            media,
            profile,
            quality_file,
            bitrate_file,
            bitrate,
            main_window: window.window_handle().downcast().unwrap(),
            fullscreen: false,
            feedback: None,
            feedback_text: String::new(),
            capturing: false,
            report: None,
            capture_message: String::new(),
            quick_open: false,
            pinned: false,
            leave_at: None,
            previous_x: -1,
            previous_multiplier: 2.,
            bitrate_menu_open: false,
            setting_menu: None,
            fields,
            values,
            page: if preview.is_some() {
                Page::Stream
            } else {
                Page::Session
            },
            inputs: Default::default(),
            selected: 0,
            busy: false,
            status: 0,
            effect_status: 0,
            notice,
            focus: cx.focus_handle(),
            discovery: discovery::State::Searching,
        };
        if preview.is_none() {
            portal.scan(cx);
        }
        portal
    }
    fn scan(&mut self, cx: &mut Context<Self>) {
        self.discovery = discovery::State::Searching;
        let task = cx.background_executor().spawn(async { discovery::scan() });
        cx.spawn(async move |this, cx| {
            let results = task.await;
            let _ = this.update(cx, |portal, cx| {
                portal.discovery = results;
                cx.notify();
            });
        })
        .detach();
    }

    fn act(&mut self, action: Action, w: &mut Window, cx: &mut Context<Self>) {
        if Window::window_handle(w) != self.main_window.into() {
            let main = self.main_window;
            cx.defer(move |cx| {
                let _ = main.update(cx, |p, w, cx| p.act(action, w, cx));
            });
            return;
        }
        self.notice.clear();
        if !matches!(action, Action::ToggleBitrateMenu) {
            self.bitrate_menu_open = false;
        }
        if !matches!(action, Action::SettingMenu(_)) {
            self.setting_menu = None;
        }
        let file: Vec<u16> = self
            .profile
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let action = if let Action::ChooseSetting(i, value) = action {
            Action::Change(i, (value - self.values[i]) / self.fields[i].step)
        } else if matches!(action, Action::ToggleFg) {
            let i = self
                .fields
                .iter()
                .position(|f| f.path == "multiplier")
                .unwrap();
            Action::Change(
                i,
                if self.values[i] > 1. {
                    self.previous_multiplier = self.values[i];
                    1. - self.values[i]
                } else {
                    self.previous_multiplier - 1.
                },
            )
        } else {
            action
        };
        match action {
            Action::ToggleFg | Action::ChooseSetting(..) => unreachable!(),
            Action::CaptureCadence => {
                if self.capturing {
                    return;
                }
                let root = self.quality_file.parent().unwrap().to_owned();
                let sidecar = self.motion.pid();
                self.capturing = true;
                self.capture_message.clear();
                session_event(&format!(
                    "capture=start rendererPid={} sidecarPid={sidecar:?}",
                    std::process::id()
                ));
                let work = cx
                    .background_executor()
                    .spawn(async move { cadence::capture(&root, sidecar) });
                cx.spawn(async move |this, cx| {
                    let result = work.await;
                    let _ = this.update(cx, |p, cx| {
                        p.capturing = false;
                        match result {
                            Ok((path, lost)) => {
                                session_event(&format!("capture=saved path={}", path.display()));
                                p.report = Some(path);
                                p.capture_message = if lost == 0 {
                                    "Medição concluída".into()
                                } else {
                                    format!("Captura parcial · {lost} eventos perdidos")
                                };
                            }
                            Err(e) => {
                                session_event(&format!("capture=failed error={e}"));
                                p.capture_message = e;
                            }
                        }
                        cx.notify();
                    });
                })
                .detach();
            }
            Action::OpenReport => {
                if let Some(dir) = &self.report {
                    cx.open_with_system(dir);
                }
            }
            Action::PinQuick => {
                self.pinned = !self.pinned;
                self.leave_at = None;
            }
            Action::CloseQuick => {
                self.pinned = false;
                self.quick_open = false;
                self.focus.focus(w);
            }
            Action::Fullscreen => {
                w.toggle_fullscreen();
                self.quick_open = self.pinned;
                self.focus.focus(w);
            }
            Action::SettingMenu(i) => {
                self.setting_menu = (self.setting_menu != Some(i)).then_some(i)
            }
            Action::ManualPair => {
                self.inputs[0].clear();
                self.selected = 0;
                self.page = Page::Pair;
                self.focus.focus(w);
            }
            Action::AccountHelp => cx.open_url("https://pyrosoft.pro/psnid/"),
            Action::Scan => {
                if !matches!(self.discovery, discovery::State::Searching) {
                    self.scan(cx);
                }
            }
            Action::SelectConsole(index) => {
                if !self.busy
                    && let discovery::State::Found(consoles) = &self.discovery
                    && let Some(console) = consoles.get(index)
                {
                    self.inputs[0].clone_from(&console.address);
                    self.inputs[2].clear();
                    self.selected = 1;
                    self.page = Page::Pair;
                    self.focus.focus(w);
                }
            }
            Action::Page(page) => {
                if self.page == Page::Stream && page != Page::Stream && w.is_fullscreen() {
                    w.toggle_fullscreen();
                }
                if page == Page::Pair {
                    self.selected = if let discovery::State::Found(consoles) = &self.discovery
                        && consoles.len() == 1
                    {
                        self.inputs[0].clone_from(&consoles[0].address);
                        1
                    } else {
                        0
                    };
                }
                if page == Page::Session && self.page.is_dialog() {
                    self.inputs[2].clear();
                    if self.page == Page::Login && !self.media.0.is_null() {
                        unsafe { portal_stop(self.media.0) };
                    }
                }
                self.page = page;
                if page == Page::Pair {
                    self.focus.focus(w)
                }
            }
            Action::Edit(i) => {
                self.selected = i;
                self.focus.focus(w);
            }
            Action::Pair if !self.busy => {
                let [host, id, pin] = self.inputs.clone();
                if host.trim().is_empty()
                    || id.trim().is_empty()
                    || pin.len() != 8
                    || !pin.bytes().all(|b| b.is_ascii_digit())
                {
                    self.notice = "Informe o IP, o Account ID e os oito dígitos do código.".into()
                } else {
                    self.inputs[2].clear();
                    self.busy = true;
                    self.notice = "Pareando…".into();
                    let task = cx
                        .background_executor()
                        .spawn(async move { pairing::register(&file, &host, &id, &pin) });
                    cx.spawn(async move|this,cx|{let rc=task.await;let _=this.update(cx,|p,cx|{
        p.busy=false;if rc==0{p.page=Page::Session;p.notice="PS5 pareado.".into()}else{p.notice=format!("Falha no pareamento ({rc}). Confira o IP, o Account ID e o código.");}cx.notify();
      });}).detach();
                }
            }
            Action::Pair => {}
            Action::Login => {
                let pin = std::mem::take(&mut self.inputs[2]);
                if !pin.is_empty() && pin.bytes().all(|b| b.is_ascii_digit()) {
                    let digits = std::ffi::CString::new(pin).unwrap();
                    unsafe { portal_login(self.media.0, digits.as_ptr()) }
                } else {
                    self.notice = "Informe o PIN de login.".into();
                }
            }
            _ if self.media.0.is_null() => self.notice = "Motor de vídeo indisponível".into(),
            Action::Connect | Action::Wake => {
                let wake = matches!(action, Action::Wake);
                if !self.profile.is_file() {
                    self.page = Page::Pair;
                    self.focus.focus(w);
                    self.notice = "Pareie o console neste aplicativo.".into();
                } else if unsafe {
                    portal_open(self.media.0, file.as_ptr(), wake, self.bitrate * 1000)
                } != 0
                {
                    self.notice = "Falha ao acessar o PS5.".into()
                } else if wake {
                    self.notice = "Comando enviado.".into()
                } else {
                    self.page = Page::Stream
                }
            }
            Action::Stop => {
                if w.is_fullscreen() {
                    w.toggle_fullscreen();
                }
                unsafe { portal_stop(self.media.0) }
            }
            Action::ToggleBitrateMenu => {
                self.bitrate_menu_open = !self.bitrate_menu_open;
            }
            Action::Bitrate(requested) => {
                match std::fs::write(&self.bitrate_file, requested.to_string()) {
                    Ok(()) => self.bitrate = requested,
                    Err(err) => self.notice = format!("Falha ao salvar o bitrate: {err}"),
                }
            }
            Action::Change(i, d) => {
                let f = &self.fields[i];
                let old = self.values.clone();
                self.values[i] =
                    ((old[i] + d * f.step).clamp(f.min, f.max) * 10000.).round() / 10000.;
                quality::normalize(&self.fields, &mut self.values);
                session_event(&format!(
                    "request={} previous={} requested={}",
                    f.path, old[i], self.values[i]
                ));
                self.feedback = None;
                self.feedback_text.clear();
                if self.setting_value("frameGenerationBackend") as u32
                    >= super_motion::FIRST_EXTERNAL
                    && super_motion::runtime(self.setting_value("frameGenerationBackend") as u32)
                        .is_err()
                    && (f.path == "frameGenerationBackend"
                        || (f.path == "multiplier" && self.values[i] > 1.))
                {
                    self.values = old;
                    self.notice =
                        "Instale o pacote Next Boost correspondente ao gerador para usar esse método.".into();
                    cx.notify();
                    return;
                }
                if !unsafe { portal_quality(self.media.0, self.values.as_ptr(), self.values.len()) }
                {
                    session_event(&format!(
                        "effect={} result=rejected retained={}",
                        f.path, old[i]
                    ));
                    self.values = old;
                    self.notice = "Combinação de parâmetros inválida.".into();
                } else {
                    if matches!(self.status, 2..=3) {
                        self.feedback = Some((i, std::time::Instant::now()));
                        self.feedback_text = "Aplicando…".into();
                    }
                    let saved: std::collections::BTreeMap<_, _> = self
                        .fields
                        .iter()
                        .map(|f| f.path)
                        .zip(self.values.iter())
                        .collect();
                    if let Err(e) = std::fs::write(
                        &self.quality_file,
                        serde_json::to_vec_pretty(&saved).unwrap(),
                    ) {
                        self.notice = e.to_string();
                    }
                }
            }
        }
        cx.notify();
    }
    fn input(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if (self.bitrate_menu_open || self.setting_menu.is_some()) && e.keystroke.key == "escape" {
            self.setting_menu = None;
            self.bitrate_menu_open = false;
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if window.is_fullscreen() && !self.quick_open && e.keystroke.key == "escape" {
            window.toggle_fullscreen();
            cx.stop_propagation();
            return;
        }
        if self.page == Page::Stream && matches!(self.status, 2..=3) {
            match e.keystroke.key.as_str() {
                "f11" => self.act(Action::Fullscreen, window, cx),
                "escape" if self.quick_open => self.act(Action::CloseQuick, window, cx),
                "escape" if window.is_fullscreen() => self.act(Action::Fullscreen, window, cx),
                "f10" => {
                    self.pinned = false;
                    self.quick_open = !self.quick_open;
                    self.setting_menu = None;
                    cx.notify();
                }
                _ => return,
            }
            cx.stop_propagation();
            return;
        }
        if !matches!(self.page, Page::Pair | Page::Login) || self.busy {
            return;
        }
        let k = &e.keystroke;
        match k.key.as_str() {
            "escape" => self.act(Action::Page(Page::Session), window, cx),
            "enter" => self.act(
                if self.page == Page::Pair {
                    Action::Pair
                } else {
                    Action::Login
                },
                window,
                cx,
            ),
            "tab" if self.page == Page::Pair => {
                self.selected = (self.selected + if k.modifiers.shift { 2 } else { 1 }) % 3;
                cx.stop_propagation();
            }
            "backspace" => {
                self.inputs[self.selected].pop();
            }
            _ => {
                let text = if k.modifiers.control && k.key.eq_ignore_ascii_case("v") {
                    cx.read_from_clipboard().and_then(|i| i.text())
                } else if !k.modifiers.control && !k.modifiers.alt {
                    k.key_char.clone()
                } else {
                    None
                };
                if let Some(text) = text {
                    let field = &mut self.inputs[self.selected];
                    let max = [253usize, 32, 8][self.selected];
                    field.extend(
                        text.chars()
                            .filter(|c| {
                                c.is_ascii_graphic() && (self.selected != 2 || c.is_ascii_digit())
                            })
                            .take(max.saturating_sub(field.len())),
                    );
                }
            }
        }
        cx.notify();
    }
    fn button<T: Into<SharedString>>(
        id: usize,
        text: T,
        act: Action,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let (base, hover) = match act {
            Action::Connect | Action::Login | Action::Pair | Action::Page(Page::Pair) => {
                (ACCENT, ACCENT_HOVER)
            }
            Action::Page(Page::Session | Page::Quality) | Action::Scan => (PANEL, BUTTON),
            _ => (BUTTON, BUTTON_HOVER),
        };
        div()
            .id(("action", id))
            .px_3()
            .when(matches!(act, Action::ChooseSetting(..)), |d| {
                d.py_0p5().text_sm()
            })
            .when(!matches!(act, Action::ChooseSetting(..)), |d| d.py_2())
            .rounded_lg()
            .border_1()
            .border_color(rgb(if base == ACCENT { ACCENT } else { BORDER }))
            .text_color(rgb(if base == ACCENT { BACKGROUND } else { TEXT }))
            .bg(rgb(base))
            .hover(move |style| style.bg(rgb(hover)))
            .focusable()
            .focus(|style| style.border_color(rgb(FOCUS)))
            .on_key_down(cx.listener(move |p, event: &KeyDownEvent, w, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    cx.stop_propagation();
                    p.act(act, w, cx);
                }
            }))
            .cursor_pointer()
            .child(text.into())
            .on_click(cx.listener(move |p, _, w, cx| p.act(act, w, cx)))
    }
    fn dialog(&self, cx: &Context<Self>) -> AnyElement {
        let pairing = self.page == Page::Pair;
        let title = if pairing {
            "Vincular console"
        } else {
            "PIN de login do PS5"
        };
        let mut body = div().flex().flex_col().gap_3().child(
            div()
                .text_xl()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        );
        if pairing {
            body = body.child(div().text_sm().text_color(rgb(MUTED)).child(
                "No PS5, abra Configurações → Sistema → Uso remoto. Ative o Uso remoto e escolha Vincular dispositivo.",
            ));
        }
        let fields = [
            ("Endereço do PS5", "IP ou nome na rede"),
            ("Account ID da PSN", "Decimal ou Base64"),
            (
                if pairing {
                    "Código de vinculação"
                } else {
                    "PIN de login"
                },
                "Digite os números",
            ),
        ];
        for (i, (label, hint)) in fields.iter().enumerate().skip(if pairing { 0 } else { 2 }) {
            let input = &self.inputs[i];
            let value = if input.is_empty() {
                hint.to_string()
            } else if i == 2 && !pairing {
                "•".repeat(input.len())
            } else {
                input.clone()
            };
            body = body.child(
                div().flex().flex_col().gap_1().child(*label).child(
                    div()
                        .id(("input", i))
                        .px_3()
                        .py_2()
                        .rounded_lg()
                        .border_1()
                        .border_color(rgb(if self.selected == i { FOCUS } else { BORDER }))
                        .bg(rgb(BACKGROUND))
                        .text_color(rgb(if input.is_empty() { MUTED } else { TEXT }))
                        .cursor_text()
                        .child(value)
                        .on_click(cx.listener(move |p, _, w, cx| p.act(Action::Edit(i), w, cx))),
                ),
            );
        }
        if pairing {
            body = body.child(Self::button(
                1002,
                "Encontrar meu Account ID no Pyrosoft ↗",
                Action::AccountHelp,
                cx,
            ));
        }
        if !self.notice.is_empty() {
            body = body.child(div().text_sm().child(self.notice.clone()));
        }
        if !self.busy {
            body = body.child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .pt_2()
                    .child(Self::button(
                        1000,
                        "Cancelar",
                        Action::Page(Page::Session),
                        cx,
                    ))
                    .child(Self::button(
                        1001,
                        if pairing {
                            "Vincular console"
                        } else {
                            "Enviar PIN"
                        },
                        if pairing { Action::Pair } else { Action::Login },
                        cx,
                    )),
            );
        }
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x000000cc))
            .child(
                div()
                    .id("console-dialog")
                    .w(px(460.))
                    .max_w_full()
                    .max_h_full()
                    .overflow_y_scroll()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(PANEL))
                    .p_5()
                    .child(body),
            )
            .into_any_element()
    }
}
mod dashboard;
impl Render for Portal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if Window::window_handle(window) != self.main_window.into() {
            return div()
                .size_full()
                .font_family("Instrument Sans")
                .bg(rgb(BACKGROUND))
                .text_color(rgb(TEXT))
                .when(
                    self.quick_open
                        || (self.feedback_text.is_empty() && self.motion.final_fps.is_none()),
                    |d| d.child(self.quick_panel(cx)),
                )
                .when(
                    !self.quick_open
                        && (!self.feedback_text.is_empty() || self.motion.final_fps.is_some()),
                    |d| {
                        d.child(
                            div()
                                .w_full()
                                .h(px(48.))
                                .flex()
                                .items_center()
                                .px_4()
                                .bg(rgb(PANEL))
                                .text_sm()
                                .child(if self.feedback_text.is_empty() {
                                    format!("{} FPS", self.motion.final_fps.unwrap_or(0))
                                } else {
                                    self.feedback_text.clone()
                                }),
                        )
                    },
                )
                .into_any_element();
        }
        self.fullscreen = window.is_fullscreen();
        if (self.page.is_dialog() || self.page == Page::Stream)
            && !self.focus.contains_focused(window, cx)
        {
            self.focus.focus(window);
        }
        let page = if self.page.is_dialog() {
            Page::Session
        } else {
            self.page
        };
        let live = matches!(self.status, 1..=3);
        let mut col = div()
            .id("content")
            .min_w_0()
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_3()
            .px_8()
            .py_6();
        match page {
            Page::Pair | Page::Login => unreachable!(),
            Page::Session => {
                col = col.child(Self::page_header(
                    "Remote Play",
                    "Durante o jogo, mova o mouse à borda esquerda para abrir os ajustes rápidos.",
                ));
                let cards = div()
                    .flex()
                    .gap_3()
                    .when(window.viewport_size().width < px(900.), |d| d.flex_col())
                    .child(self.console_card(cx).flex_1())
                    .child(self.streaming_card(cx).flex_1());
                col = col.child(cards).child(self.network_card(cx));
            }
            Page::Stream => {
                if self.status == 1 {
                    col = col.child(Self::page_header(
                        "Conectando ao PS5…",
                        "A imagem aparece assim que o console iniciar a transmissão.",
                    ));
                } else if !live {
                    col = col
                        .child(self.console_card(cx))
                        .child(self.streaming_card(cx));
                }
            }
            Page::Network => {
                col = col
                    .child(Self::heading("Configurações"))
                    .child(self.network_card(cx));
            }
            Page::About => {
                col = col.child(Self::heading("Sobre")).child(Self::card(
                    "VeloraNR",
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child("Remote Play para PlayStation 5")
                        .child(format!("Versão {}", env!("CARGO_PKG_VERSION"))),
                ));
            }
            Page::Quality | Page::Dlss | Page::FrameGen => {
                let (title, subtitle) = match page {
                    Page::Dlss => (
                        "DLSS 5",
                        "Ajuste o processamento neural e as camadas de DLSS NR.",
                    ),
                    Page::FrameGen => (
                        "Frame Generation",
                        "Escolha o método e o multiplicador para a geração de quadros.",
                    ),
                    _ => (
                        "Qualidade de imagem",
                        "Ajuste o antisserrilhamento, a redução de ruído e os demais efeitos.",
                    ),
                };
                col = col.child(Self::page_header(title, subtitle));
                if page == Page::Dlss {
                    col = col
                        .child(self.effect_health(false))
                        .child(self.effect_settings("DLSS NR", cx));
                }
                if page == Page::FrameGen {
                    col = col
                        .child(self.effect_health(true))
                        .child(self.effect_settings("Frame Generation", cx));
                }
                let mut last = "";
                for (i, f) in self.fields.iter().enumerate() {
                    if f.group != last
                        && Self::effect_page(f.group) == page
                        && page != Page::FrameGen
                        && f.group != "DLSS NR"
                        && (!f.group.starts_with("Camada NR ")
                            || f.group.as_bytes().last().unwrap() - b'0'
                                <= self.setting_value("nrLayerCount") as u8)
                    {
                        col = col.child(Self::button(
                            i + 1,
                            if f.group == "Ruído e AA" {
                                "Antisserrilhamento e redução de ruído"
                            } else {
                                f.group
                            },
                            Action::Page(Page::Group(f.group)),
                            cx,
                        ));
                        last = f.group;
                    }
                }
            }
            Page::Group(group) => {
                col = col
                    .child(Self::button(
                        0,
                        "Voltar à qualidade",
                        Action::Page(Self::effect_page(group)),
                        cx,
                    ))
                    .child(self.effect_settings(group, cx));
            }
        }
        let dialog = self.page.is_dialog();
        div()
            .relative()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|v, e, w, cx| v.input(e, w, cx)))
            .size_full()
            .bg(rgb(BACKGROUND))
            .text_color(rgb(TEXT))
            .font_family("Instrument Sans")
            .child(
                div()
                    .size_full()
                    .flex()
                    .when(
                        page != Page::Stream || !matches!(self.status, 2..=3),
                        |row| row.child(self.navigation(page, cx)),
                    )
                    .child(col.when(!self.notice.is_empty(), |col| {
                        col.child(self.notice.clone())
                    })),
            )
            .when(dialog, |root| root.child(self.dialog(cx)))
            .into_any_element()
    }
}
fn main() {
    // GPUI's DirectComposition surface can cover the native DXGI child.
    // Set the renderer mode before GPUI starts any threads.
    unsafe { std::env::set_var("GPUI_DISABLE_DIRECT_COMPOSITION", "1") };
    Application::new().with_assets(dashboard::Assets).run(|cx| {
        cx.text_system()
            .add_fonts(vec![std::borrow::Cow::Borrowed(include_bytes!(
                "../assets/InstrumentSans.ttf"
            ))])
            .expect("carregar Instrument Sans");

        let b = Bounds::centered(None, size(px(1150.), px(680.)), cx);
        let main = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(b)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("VeloraNR".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |w, cx| cx.new(|cx| Portal::new(w, cx)),
            )
            .expect("janela GPUI");
        let view = main.update(cx, |_, _, cx| cx.entity()).unwrap();
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(QUICK_WIDTH as f32), px(540.)),
                    cx,
                ))),
                titlebar: None,
                focus: false,
                show: false,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                ..Default::default()
            },
            move |w, cx| {
                let RawWindowHandle::Win32(handle) =
                    HasWindowHandle::window_handle(w).unwrap().as_raw()
                else {
                    unreachable!()
                };
                let media = view.read(cx).media.0;
                if !media.is_null() {
                    unsafe { portal_attach_panel(media, handle.hwnd.get()) };
                }
                view
            },
        )
        .expect("janela de ajustes");
        cx.on_window_closed(move |cx| {
            if !cx.windows().contains(&main.into()) {
                unsafe { portal_cancel_measure() };
                cx.quit();
            }
        })
        .detach();
        let _ = main.update(cx, |_, w, _| w.activate_window());
        cx.activate(true);
    });
}
