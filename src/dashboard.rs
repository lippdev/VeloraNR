use super::*;
use std::borrow::Cow;

pub(super) struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok((path == "dualsense.svg")
            .then(|| Cow::Borrowed(include_bytes!("../assets/dualsense.svg").as_slice())))
    }

    fn list(&self, _: &str) -> Result<Vec<SharedString>> {
        Ok(vec!["dualsense.svg".into()])
    }
}

impl Portal {
    pub(super) fn page_header(title: &'static str, subtitle: &'static str) -> Div {
        div()
            .pb_3()
            .child(
                div()
                    .text_size(px(36.))
                    .font_weight(FontWeight::BOLD)
                    .child(title),
            )
            .child(div().mt_1().text_color(rgb(MUTED)).child(subtitle))
    }

    pub(super) fn effect_page(group: &str) -> Page {
        if group == "DLSS NR" || group.starts_with("Camada NR ") {
            Page::Dlss
        } else if group == "Frame Generation" {
            Page::FrameGen
        } else {
            Page::Quality
        }
    }

    pub(super) fn effect_settings(&self, group: &'static str, cx: &Context<Self>) -> Div {
        let mut body = div().flex().flex_col();
        if group == "Frame Generation" {
            body = body.child(Self::button(
                9000,
                if self
                    .fields
                    .iter()
                    .zip(&self.values)
                    .any(|(field, value)| field.path == "multiplier" && *value > 1.)
                {
                    "Desativar Frame Generation"
                } else {
                    "Ativar Frame Generation"
                },
                Action::ToggleFg,
                cx,
            ));
        }
        let backend = self
            .fields
            .iter()
            .position(|field| field.path == "frameGenerationBackend")
            .unwrap();
        for (i, field) in self
            .fields
            .iter()
            .enumerate()
            .filter(|(_, f)| f.group == group)
        {
            let value = self.values[i];
            if field.path.starts_with("vfg") && self.values[backend] != 4. {
                continue;
            }
            let (label, options): (&str, &[&str]) = match field.path {
                "frameGenerationBackend" => (
                    "Método",
                    &[
                        "NVIDIA DLSS FG",
                        "Intel XeSS FG",
                        "AMD FSR 3.1",
                        "AMD FSR 4 ML",
                        "NVIDIA VFG",
                        "ANR FG",
                        "AMD FRC",
                        "RIFE",
                        "XeFG XMX",
                    ],
                ),
                "fgMotion" | "nrMotion" | "srMotion" => (
                    "Fonte de movimento",
                    &["Sem movimento", "Fluxo óptico", "Automático"],
                ),
                "vfgQuality" => ("Qualidade VFG", &["Baixa", "Média", "Alta"]),
                "lowLatency" => (
                    "Ordem do processamento",
                    &["Depois do upscaling", "Antes do upscaling"],
                ),
                "antiAlias.method" => (
                    "Método de antisserrilhamento",
                    &["FXAA", "SMAA", "MLAA", "DLAA"],
                ),
                "antiAlias.temporal" => ("Acumulação temporal", &["Desligada", "T2x", "4x"]),
                _ => (field.label, &[]),
            };
            let toggle =
                options.is_empty() && field.min == 0. && field.max == 1. && field.step == 1.;
            let mut row = div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .py_2()
                .border_b_1()
                .border_color(rgb(BORDER))
                .child(label);
            if toggle {
                row = row.child(Self::button(
                    i * 2 + 1,
                    if value == 0. { "Desligado" } else { "Ligado" },
                    Action::Change(i, if value == 0. { 1. } else { -1. }),
                    cx,
                ));
            } else {
                let display = match field.path {
                    "multiplier" if value == 1. => "Desligado".into(),
                    "multiplier" => format!("{value:.0}×"),
                    _ => options
                        .get(value as usize)
                        .map(|label| label.to_string())
                        .unwrap_or_else(|| {
                            if field.step >= 1. {
                                format!("{value:.0}")
                            } else {
                                format!("{value:.2}")
                            }
                        }),
                };
                row = row.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Self::button(i * 2 + 1, "−", Action::Change(i, -1.), cx))
                        .child(div().min_w(px(65.)).text_center().child(display))
                        .child(Self::button(i * 2 + 2, "+", Action::Change(i, 1.), cx)),
                );
            }
            body = body.child(row);
        }
        Self::card(
            if group == "Ruído e AA" {
                "Antisserrilhamento e redução de ruído"
            } else {
                group
            },
            body,
        )
    }

    pub(super) fn heading(title: &'static str) -> Div {
        div()
            .text_xl()
            .font_weight(FontWeight::SEMIBOLD)
            .child(title)
    }

    pub(super) fn card(title: &'static str, body: Div) -> Div {
        div()
            .min_w_0()
            .flex_shrink_0()
            .border_1()
            .border_color(rgb(BORDER))
            .rounded_xl()
            .shadow_md()
            .bg(rgb(PANEL))
            .child(
                div()
                    .px_4()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .font_weight(FontWeight::BOLD)
                    .child(title),
            )
            .child(body.p_3())
    }

    pub(super) fn navigation(&self, page: Page, cx: &Context<Self>) -> Div {
        let mut nav = div().flex().flex_col().gap_1().p_2();
        for (index, (icon, label, dest)) in [
            ("\u{e80f}", "Visão geral", Page::Session),
            ("\u{e768}", "Streaming", Page::Stream),
            ("\u{e9e9}", "Qualidade de imagem", Page::Quality),
            ("\u{e9d9}", "DLSS 5", Page::Dlss),
            ("\u{e7f4}", "Frame Generation", Page::FrameGen),
            ("\u{e713}", "Configurações", Page::Network),
            ("\u{e946}", "Sobre", Page::About),
        ]
        .into_iter()
        .enumerate()
        {
            let active = page == dest
                || matches!(page, Page::Group(group) if dest == Self::effect_page(group));
            nav = nav.child(
                div()
                    .id(("navigation", index))
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .rounded_sm()
                    .when(active, |d| d.bg(rgb(BUTTON_HOVER)))
                    .hover(|d| d.bg(rgb(BUTTON_HOVER)))
                    .cursor_pointer()
                    .focusable()
                    .focus(|d| d.bg(rgb(BORDER)))
                    .on_click(cx.listener(move |p, _, w, cx| p.act(Action::Page(dest), w, cx)))
                    .on_key_down(cx.listener(move |p, e: &KeyDownEvent, w, cx| {
                        if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                            cx.stop_propagation();
                            p.act(Action::Page(dest), w, cx);
                        }
                    }))
                    .child(div().font_family("Segoe MDL2 Assets").child(icon))
                    .child(label),
            );
        }
        let mut rail = div()
            .w(px(SIDEBAR as f32))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .bg(rgb(0x18191b))
            .border_r_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .py_5()
                    .flex()
                    .px_4()
                    .gap_2()
                    .items_center()
                    .child(
                        img(self.logo.clone())
                            .size(px(30.))
                            .object_fit(ObjectFit::Contain),
                    )
                    .child(Self::heading("VeloraNR")),
            )
            .child(nav);
        if page == Page::Stream && matches!(self.status, 1..=3) {
            rail = rail.child(div().p_2().child(Self::button(
                800,
                "Desconectar",
                Action::Stop,
                cx,
            )));
        }
        rail.child(div().flex_1()).child(
            div()
                .p_4()
                .font_family("Consolas")
                .text_color(rgb(MUTED))
                .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
        )
    }

    pub(super) fn console_card(&self, cx: &Context<Self>) -> Div {
        let paired = self.profile.is_file();
        let status = match self.status {
            1 => "Conectando…",
            2 => "Em reprodução",
            3 => "Pausado",
            5 => "Desconectando…",
            6 => "Falha na sessão",
            _ if paired => "Vinculado",
            _ => "Não vinculado",
        };
        let mut body = div().flex().flex_col().gap_2().child(
            div()
                .flex()
                .items_center()
                .gap_4()
                .child(
                    div()
                        .size(px(76.))
                        .flex_shrink_0()
                        .rounded_xl()
                        .bg(rgb(BACKGROUND))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            svg()
                                .path("dualsense.svg")
                                .size(px(52.))
                                .text_color(rgb(TEXT)),
                        ),
                )
                .child(
                    div()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("PlayStation 5"),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(if paired && self.status != 6 {
                                    0x9abc99
                                } else {
                                    MUTED
                                }))
                                .child(status),
                        ),
                ),
        );
        if matches!(self.status, 1..=3) {
            body = body
                .child(Self::button(
                    0,
                    "Ver streaming",
                    Action::Page(Page::Stream),
                    cx,
                ))
                .child(Self::button(1, "Desconectar", Action::Stop, cx));
        } else if paired {
            body = body
                .child(Self::button(0, "Conectar", Action::Connect, cx))
                .child(Self::button(2, "Acordar console", Action::Wake, cx));
        } else {
            body = body.child(Self::button(
                0,
                "Vincular console",
                Action::Page(Page::Pair),
                cx,
            ));
        }
        Self::card(
            if paired {
                "PS5 pareado"
            } else {
                "Pareamento do PS5"
            },
            body,
        )
    }

    pub(super) fn streaming_card(&self, cx: &Context<Self>) -> Div {
        let mut bitrate = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child("Bitrate");
        if matches!(self.status, 1..=3) {
            bitrate = bitrate.child(format!("{} Mbps", self.bitrate));
        } else {
            bitrate = bitrate.child(Self::button(
                6,
                format!("{} Mbps  ▾", self.bitrate),
                Action::ToggleBitrateMenu,
                cx,
            ));
        }
        let mut body = div().flex().flex_col().gap_2().child(bitrate);
        if self.bitrate_menu_open {
            let mut menu = div().flex().flex_col().border_1().border_color(rgb(BORDER));
            for (i, value) in BITRATES.into_iter().enumerate() {
                let mark = if value == self.bitrate { "  ✓" } else { "" };
                menu = menu.child(
                    div()
                        .id(("bitrate", i))
                        .px_3()
                        .text_sm()
                        .bg(rgb(BUTTON))
                        .hover(|d| d.bg(rgb(BUTTON_HOVER)))
                        .cursor_pointer()
                        .focusable()
                        .focus(|d| d.bg(rgb(BORDER)))
                        .child(format!("{value} Mbps{mark}"))
                        .on_key_down(cx.listener(move |p, e: &KeyDownEvent, w, cx| {
                            if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                                cx.stop_propagation();
                                p.act(Action::Bitrate(value), w, cx);
                            }
                        }))
                        .on_click(
                            cx.listener(move |p, _, w, cx| p.act(Action::Bitrate(value), w, cx)),
                        ),
                );
            }
            body = body.child(menu);
        }
        for (path, label) in [("nr", "DLSS 5"), ("multiplier", "Frame Generation")] {
            if let Some(i) = self.fields.iter().position(|f| f.path == path) {
                let enabled = self.values[i] > if path == "multiplier" { 1. } else { 0. };
                body = body.child(
                    div()
                        .flex()
                        .justify_between()
                        .items_center()
                        .border_t_1()
                        .border_color(rgb(BORDER))
                        .pt_2()
                        .child(label)
                        .child(Self::button(
                            7000 + i,
                            if enabled {
                                "●  Ligado"
                            } else {
                                "○  Desligado"
                            },
                            if path == "multiplier" {
                                Action::ToggleFg
                            } else {
                                Action::Change(i, if enabled { -1. } else { 1. })
                            },
                            cx,
                        )),
                );
            }
        }
        Self::card("Qualidade de imagem", body)
    }

    pub(super) fn network_card(&self, cx: &Context<Self>) -> Div {
        let mut list = div().flex().flex_col().gap_2().flex_1();
        match &self.discovery {
            discovery::State::Searching => list = list.child("Buscando PS5 na rede local…"),
            discovery::State::Found(consoles) => {
                for (index, console) in consoles.iter().enumerate() {
                    let state = if console.standby {
                        "Em repouso"
                    } else {
                        "Ligado"
                    };
                    list = list.child(Self::button(
                        20 + index,
                        format!("{}  ·  {state}", console.address),
                        Action::SelectConsole(index),
                        cx,
                    ));
                }
            }
            discovery::State::Empty => list = list.child("Nenhum PS5 encontrado na rede local."),
            discovery::State::Failed(code) => {
                list = list.child(format!("A busca na rede falhou ({code})."))
            }
        }
        Self::card(
            "Consoles na rede",
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(list)
                .when(
                    !matches!(self.discovery, discovery::State::Searching),
                    |d| d.child(Self::button(5, "Buscar novamente", Action::Scan, cx)),
                )
                .child(Self::button(
                    3,
                    "Informar IP manualmente",
                    Action::Page(Page::Pair),
                    cx,
                )),
        )
    }
}
