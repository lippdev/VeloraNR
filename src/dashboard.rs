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

    pub(super) fn effect_health(&self, fg: bool) -> Div {
        if !matches!(self.status, 2..=3) {
            return div();
        }
        if fg && self.setting_value("frameGenerationBackend") as u32 == super_motion::BACKEND {
            let label = if !self.motion.status.is_empty() {
                self.motion.status.as_str()
            } else if self.setting_value("multiplier") <= 1. {
                "Super Motion desligado"
            } else {
                "Super Motion preparado · abra a transmissão"
            };
            return div()
                .text_sm()
                .text_color(rgb(MUTED))
                .child(label.to_owned());
        }
        let multiplier = self.effect_status & 15;
        let requested = self.setting_value("multiplier") as i32;
        let nr_active = self.effect_status & 16 != 0;
        let status = if self.effect_status & 32 != 0 {
            "Aplicando configuração…".into()
        } else if fg && requested > 1 && multiplier != requested {
            format!(
                "Solicitado {requested}× · aplicado {}×. Consulte o log para detalhes.",
                multiplier.max(1)
            )
        } else if fg && multiplier > 1 {
            format!("Ativo · {multiplier}× aplicado")
        } else if !fg && nr_active {
            "DLSS NR ativo no vídeo".into()
        } else if !fg && self.setting_value("nr") != 0. {
            "DLSS NR solicitado · aguardando confirmação do motor".into()
        } else {
            "Desligado na transmissão".into()
        };
        div().text_sm().text_color(rgb(MUTED)).child(status)
    }

    pub(super) fn setting_value(&self, path: &str) -> f32 {
        let index = self.fields.iter().position(|f| f.path == path).unwrap();
        self.values[index]
    }

    pub(super) fn quick_panel(&self, window: &Window, cx: &Context<Self>) -> impl IntoElement {
        div()
            .id("quick-settings")
            .w(px(QUICK_WIDTH as f32))
            .h_full()
            .flex_shrink_0()
            .overflow_y_scroll()
            .bg(rgb(BACKGROUND))
            .px_3()
            .py_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(Self::effect_settings(self, "Ajustes rápidos", cx))
                    .child(self.effect_health(true))
                    .when(!self.notice.is_empty(), |d| d.child(self.notice.clone()))
                    .child(Self::button(
                        9101,
                        if window.is_fullscreen() {
                            "Sair da tela cheia · F11"
                        } else {
                            "Tela cheia · F11"
                        },
                        Action::Fullscreen,
                        cx,
                    ))
                    .child(Self::button(
                        9102,
                        "Voltar ao jogo · Esc",
                        Action::CloseQuick,
                        cx,
                    ))
                    .child(Self::button(
                        9103,
                        "Visão geral",
                        Action::Page(Page::Session),
                        cx,
                    ))
                    .child(Self::button(9104, "Desconectar", Action::Stop, cx)),
            )
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
        let backend = self.setting_value("frameGenerationBackend") as u32;
        let aa_method = self.setting_value("antiAlias.method") as u32;
        for (i, field) in self.fields.iter().enumerate().filter(|(_, f)| {
            if group == "Ajustes rápidos" {
                match f.path {
                    "nr" | "sr" | "multiplier" | "antiAlias.enabled" => true,
                    "srTarget" => self.setting_value("sr") != 0.,
                    "antiAlias.method" => self.setting_value("antiAlias.enabled") != 0.,
                    _ => false,
                }
            } else {
                f.group == group
            }
        }) {
            let value = self.values[i];
            let applicable = match field.path {
                _ if group == "Frame Generation" && backend == super_motion::BACKEND => {
                    matches!(field.path, "frameGenerationBackend" | "multiplier")
                }
                "vfgQuality" | "vfgAutoRegulate" => backend == 4,
                "fgMotion" => !matches!(backend, 4 | 6 | 7),
                "hudLock" => backend != 1,
                "antiAlias.quality" => aa_method <= 1,
                "antiAlias.temporal" => aa_method == 1,
                "antiAlias.historyWeight" | "antiAlias.historyRejection" => {
                    aa_method == 1 && self.setting_value("antiAlias.temporal") > 0.
                }
                "antiAlias.mlaaThreshold" | "antiAlias.mlaaStrength" => aa_method == 2,
                _ => true,
            };
            if !applicable {
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
                        "Super Motion (Next Boost)",
                    ],
                ),
                "fgMotion" | "nrMotion" | "srMotion" => (
                    "Fonte de movimento",
                    &["Sem movimento", "Fluxo óptico", "Automático"],
                ),
                "vfgQuality" => (
                    "Qualidade VFG",
                    &[
                        "Baixa",
                        "Média",
                        "Alta",
                        "Alta + média",
                        "Alta + baixa",
                        "Média + baixa",
                    ],
                ),
                "multiplier" => (
                    "Multiplicador",
                    &["Desligado", "2×", "3×", "4×", "5×", "6×", "7×", "8×"]
                        [..quality::fg_limit(backend) as usize],
                ),
                "antiAlias.quality" if aa_method == 0 => {
                    ("Qualidade AA", &["Média", "Alta", "Ultra"])
                }
                "antiAlias.quality" => ("Qualidade AA", &["Baixa", "Média", "Alta", "Ultra"]),
                "nrReuse" => (
                    "Reutilização",
                    &[
                        "Desligada",
                        "Automática",
                        "A cada 2 quadros",
                        "A cada 3 quadros",
                        "A cada 4 quadros",
                    ],
                ),
                "srTarget" | "dualSrTarget" => (
                    "Tamanho alvo",
                    &["1440p · QHD", "4K", "8K", "5K", "6K", "7K"],
                ),
                "srOutput" => (
                    "Saída após SR",
                    &[
                        "Manter alvo",
                        "Resolução original",
                        "1440p",
                        "4K",
                        "5K",
                        "6K",
                        "7K",
                    ],
                ),
                "srPrescale" => (
                    "Pré-escala",
                    &["Desligada", "720p", "900p", "1080p", "1440p"],
                ),
                "videoSrQuality" | "dualSrQuality" => (
                    "Algoritmo",
                    &[
                        "DLSS",
                        "RTX · Baixo",
                        "RTX · Médio",
                        "RTX · Alto",
                        "RTX · Ultra",
                        "FSR",
                    ],
                ),
                _ if field.path.ends_with(".sizePolicy") => (
                    "Resolução interna",
                    &["1080p · padrão", "Nativa", "480p", "720p", "900p", "1440p"],
                ),
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
            let label = if group == "Ajustes rápidos" {
                match field.path {
                    "nr" => "DLSS 5",
                    "sr" => "Super-resolução",
                    "multiplier" => "Frame Gen",
                    "antiAlias.enabled" => "Antisserrilhamento",
                    "antiAlias.method" => "Filtro",
                    _ => label,
                }
            } else {
                label
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
            } else if !options.is_empty() {
                let selected = (value - field.min) as usize;
                row = row.child(Self::button(
                    i * 2 + 1,
                    format!(
                        "{}  ▾",
                        options.get(selected).copied().unwrap_or("Selecionar")
                    ),
                    Action::SettingMenu(i),
                    cx,
                ));
                body = body.child(row);
                if self.setting_menu == Some(i) {
                    let mut menu = div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .p_2()
                        .rounded_lg()
                        .bg(rgb(BACKGROUND));
                    for (option, label) in options.iter().enumerate() {
                        let marker = if option == selected { "  ✓" } else { "" };
                        menu = menu.child(Self::button(
                            20_000 + i * 16 + option,
                            format!("{label}{marker}"),
                            Action::ChooseSetting(i, field.min + option as f32),
                            cx,
                        ));
                    }
                    body = body.child(menu);
                }
                continue;
            } else {
                let display = if field.step >= 1. {
                    format!("{value:.0}")
                } else {
                    format!("{value:.2}")
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
            if self.setting_value("frameGenerationBackend") as u32 == super_motion::BACKEND {
                rail = rail
                    .child(div().p_2().child(Self::button(
                        801,
                        if self.setting_value("multiplier") > 1. {
                            "Desligar Super Motion"
                        } else {
                            "Ligar Super Motion"
                        },
                        Action::ToggleFg,
                        cx,
                    )))
                    .child(div().px_2().child(self.effect_health(true)));
            }
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
        let mut body = div().flex().flex_col().justify_between().gap_4().child(
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
            body = body.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Self::button(0, "Conectar", Action::Connect, cx)
                            .flex_1()
                            .text_center(),
                    )
                    .child(
                        Self::button(2, "Acordar console", Action::Wake, cx)
                            .flex_1()
                            .text_center(),
                    ),
            );
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
            div().flex().flex_col().gap_3().child(list).child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(Self::button(3, "Adicionar por IP", Action::ManualPair, cx))
                    .when(
                        !matches!(self.discovery, discovery::State::Searching),
                        |d| d.child(Self::button(5, "Buscar novamente", Action::Scan, cx)),
                    ),
            ),
        )
    }
}
