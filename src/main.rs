#![windows_subsystem = "windows"]
mod discovery;
mod pairing;
mod quality;

const BACKGROUND: u32 = 0x101820;
const PANEL: u32 = 0x1b2835;
const TEXT: u32 = 0xedf2fa;
const MUTED: u32 = 0xaab9ca;
const BUTTON: u32 = 0x26374a;
const BUTTON_HOVER: u32 = 0x354e67;
const ACCENT: u32 = 0x2761b5;
const ACCENT_HOVER: u32 = 0x3674ce;
const BORDER: u32 = 0x43566c;
const FOCUS: u32 = 0xaecbff;
const SIDEBAR: i32 = 325;
use gpui::{prelude::*, *};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{ffi::c_void, os::windows::ffi::OsStrExt, path::PathBuf, time::Duration};
unsafe extern "C" {
    fn portal_login(media: *mut c_void, pin: *const std::ffi::c_char);
    fn portal_new(parent: isize, log: *const u16) -> *mut c_void;
    fn portal_free(media: *mut c_void);
    fn portal_open(media: *mut c_void, file: *const u16, wake: bool) -> i32;
    fn portal_stop(media: *mut c_void);
    fn portal_tick(media: *mut c_void, width: i32, show: bool) -> i32;
    fn portal_quality(media: *mut c_void, values: *const f32, count: usize) -> bool;
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
    Session,
    Pair,
    Quality,
    Group(&'static str),
}
#[derive(Clone, Copy)]
enum Action {
    Login,
    Scan,
    SelectConsole(usize),
    Page(Page),
    Edit(usize),
    Pair,
    Connect,
    Stop,
    Wake,
    Change(usize, f32),
}
struct Portal {
    media: Media,
    profile: PathBuf,
    quality_file: PathBuf,
    fields: Vec<quality::Field>,
    values: Vec<f32>,
    page: Page,
    inputs: [String; 3],
    selected: usize,
    busy: bool,
    status: i32,
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
        let root = PathBuf::from(std::env::var_os("LOCALAPPDATA").expect("LOCALAPPDATA"))
            .join("PortalPS5");
        let mut notice = std::fs::create_dir_all(&root)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        let profile = root.join("console.dat");
        let quality_file = root.join("quality.json");
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
            if !unsafe { portal_quality(media.0, values.as_ptr(), values.len()) } {
                values = fields.iter().map(|f| f.initial).collect();
                if !unsafe { portal_quality(media.0, values.as_ptr(), values.len()) } {
                    notice = "Configuração inicial recusada.".into();
                }
            }
        }
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(12))
                    .await;
                if this
                    .update(cx, |p, cx| {
                        if !p.media.0.is_null() {
                            let state = unsafe {
                                portal_tick(
                                    p.media.0,
                                    SIDEBAR,
                                    !p.page.is_dialog() && matches!(p.status, 1..=3),
                                )
                            };
                            if state != p.status {
                                p.status = state;
                                if state == 7 {
                                    p.page = Page::Login;
                                    p.selected = 2;
                                    p.inputs[2].clear();
                                } else if p.page == Page::Login {
                                    p.page = Page::Session;
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
            media,
            profile,
            quality_file,
            fields,
            values,
            page: Page::Session,
            inputs: Default::default(),
            selected: 0,
            busy: false,
            status: 0,
            notice,
            focus: cx.focus_handle(),
            discovery: discovery::State::Searching,
        };
        portal.scan(cx);
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
        self.notice.clear();
        let file: Vec<u16> = self
            .profile
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        match action {
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
                if page == Page::Pair {
                    self.selected = 0;
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
                } else if unsafe { portal_open(self.media.0, file.as_ptr(), wake) } != 0 {
                    self.notice = "Falha ao acessar o PS5.".into()
                } else if wake {
                    self.notice = "Comando enviado.".into()
                }
            }
            Action::Stop => unsafe { portal_stop(self.media.0) },
            Action::Change(i, d) => {
                let f = &self.fields[i];
                let old = self.values[i];
                self.values[i] = ((old + d * f.step).clamp(f.min, f.max) * 10000.).round() / 10000.;
                if !unsafe { portal_quality(self.media.0, self.values.as_ptr(), self.values.len()) }
                {
                    self.values[i] = old;
                    self.notice = "Combinação de parâmetros inválida.".into();
                } else {
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
    ) -> impl IntoElement + use<T> {
        let (base, hover) = match act {
            Action::Connect | Action::Login | Action::Pair | Action::SelectConsole(_) => {
                (ACCENT, ACCENT_HOVER)
            }
            Action::Page(Page::Session | Page::Quality) | Action::Scan => (PANEL, BUTTON),
            _ => (BUTTON, BUTTON_HOVER),
        };
        div()
            .id(("action", id))
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(base))
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
            "Parear PS5"
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
                "No PS5, abra Configurações → Sistema → Uso remoto → Vincular dispositivo.",
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
                        .rounded_md()
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
                            "Parear console"
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
            .bg(rgba(0x060a10c9))
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
impl Render for Portal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.page.is_dialog() && !self.focus.contains_focused(window, cx) {
            self.focus.focus(window);
        }
        let mut col = div()
            .id("controls")
            .h_full()
            .overflow_y_scroll()
            .w(px(SIDEBAR as f32))
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Portal PS5"),
            );
        match if self.page.is_dialog() {
            Page::Session
        } else {
            self.page
        } {
            Page::Pair | Page::Login => unreachable!(),
            Page::Session => {
                let paired = self.profile.is_file();
                col = col.child(
                    div()
                        .text_sm()
                        .text_color(rgb(MUTED))
                        .child(match self.status {
                            1 => "Conectando ao PS5…",
                            2 => "Remote Play em reprodução",
                            3 => "Remote Play pausado",
                            5 => "Desconectando…",
                            6 => "Falha na sessão",
                            _ if paired => "PS5 pareado",
                            _ => "Configure seu PS5 para começar",
                        }),
                );
                if matches!(self.status, 1..=3) {
                    col = col.child(Self::button(1, "Desconectar", Action::Stop, cx));
                } else if paired {
                    col = col
                        .child(Self::button(
                            0,
                            "Conectar ao PS5 pareado",
                            Action::Connect,
                            cx,
                        ))
                        .child(Self::button(2, "Acordar PS5 pareado", Action::Wake, cx));
                }
                col = col.child(
                    div()
                        .mt_3()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Consoles na rede"),
                );
                match &self.discovery {
                    discovery::State::Searching => {
                        col = col.child("Buscando PS5 na rede local…");
                    }
                    discovery::State::Found(consoles) => {
                        for (index, console) in consoles.iter().enumerate() {
                            let status = if console.standby {
                                "Em repouso"
                            } else {
                                "Ligado"
                            };
                            col = col.child(Self::button(
                                20 + index,
                                format!("{} ({status})", console.address),
                                Action::SelectConsole(index),
                                cx,
                            ));
                        }
                        col = col.child(
                            div()
                                .text_sm()
                                .text_color(rgb(MUTED))
                                .child("Escolha o PS5 que deseja parear."),
                        );
                    }
                    discovery::State::Empty => {
                        col = col.child("Nenhum PS5 encontrado. Confira se o console está ligado e na mesma rede.");
                    }
                    discovery::State::Failed(code) => {
                        col = col.child(format!("A busca na rede falhou ({code})."));
                    }
                }
                if !matches!(self.discovery, discovery::State::Searching) {
                    col = col.child(Self::button(5, "Buscar novamente", Action::Scan, cx));
                }
                col = col
                    .child(Self::button(
                        3,
                        "Informar IP manualmente",
                        Action::Page(Page::Pair),
                        cx,
                    ))
                    .child(div().mt_3().child(Self::button(
                        4,
                        "Qualidade de imagem",
                        Action::Page(Page::Quality),
                        cx,
                    )));
            }
            Page::Quality => {
                col = col.child(Self::button(
                    0,
                    "Voltar ao console",
                    Action::Page(Page::Session),
                    cx,
                ));
                let mut last = "";
                for (i, f) in self.fields.iter().enumerate() {
                    if f.group != last {
                        col = col.child(Self::button(
                            i + 1,
                            f.group,
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
                        Action::Page(Page::Quality),
                        cx,
                    ))
                    .child(group);
                for (i, f) in self
                    .fields
                    .iter()
                    .enumerate()
                    .filter(|(_, f)| f.group == group)
                {
                    let v = self.values[i];
                    let toggle = f.min == 0. && f.max == 1. && f.step == 1.;
                    if toggle {
                        col = col.child(Self::button(
                            i * 2 + 1,
                            format!("{} {}", if v == 0. { "○" } else { "✓" }, f.label),
                            Action::Change(i, if v == 0. { 1. } else { -1. }),
                            cx,
                        ));
                    } else {
                        col = col.child(f.label).child(
                            div()
                                .flex()
                                .gap_2()
                                .child(Self::button(i * 2 + 1, "−", Action::Change(i, -1.), cx))
                                .child(format!("{v:.2}"))
                                .child(Self::button(i * 2 + 2, "+", Action::Change(i, 1.), cx)),
                        );
                    }
                }
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
            .when(!dialog, |root| {
                root.child(col.when(!self.notice.is_empty(), |col| {
                    col.child(self.notice.clone())
                }))
            })
            .when(dialog, |root| root.child(self.dialog(cx)))
    }
}
fn main() {
    // GPUI's DirectComposition surface can cover the native DXGI child.
    // Set the renderer mode before GPUI starts any threads.
    unsafe { std::env::set_var("GPUI_DISABLE_DIRECT_COMPOSITION", "1") };
    Application::new().run(|cx| {
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit()
            }
        })
        .detach();
        let b = Bounds::centered(None, size(px(1150.), px(680.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(b)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Portal PS5".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |w, cx| cx.new(|cx| Portal::new(w, cx)),
        )
        .expect("janela GPUI");
        cx.activate(true);
    });
}
