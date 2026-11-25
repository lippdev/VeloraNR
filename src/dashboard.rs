use super::*;

impl Portal {
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
            .rounded_sm()
            .bg(rgb(PANEL))
            .child(
                div()
                    .px_4()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .font_family("Consolas")
                    .font_weight(FontWeight::BOLD)
                    .child(title),
            )
            .child(body.p_3())
    }

    pub(super) fn navigation(&self, page: Page, cx: &Context<Self>) -> Div {
        let mut nav = div().flex().flex_col().gap_1().p_2();
        for (index, (icon, label, dest)) in [
            ("\u{e80f}", "Início", Page::Session),
            ("\u{e768}", "Streaming", Page::Stream),
            ("\u{e9e9}", "Qualidade", Page::Quality),
            ("\u{e713}", "Configurações", Page::Network),
            ("\u{e946}", "Sobre", Page::About),
        ]
        .into_iter()
        .enumerate()
        {
            let active = page == dest || matches!(page, Page::Group(_)) && dest == Page::Quality;
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
            .border_r_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .py_5()
                    .flex()
                    .flex_col()
                    .items_center()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .font_family("Consolas")
                    .child(div().text_3xl().child("Ⅴ"))
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
                .child("PS5"),
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
            _ if paired => "Pareado",
            _ => "Aguardando pareamento",
        };
        let mut body = div().flex().flex_col().gap_2().child(
            div()
                .flex()
                .items_center()
                .gap_4()
                .child(
                    div()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .px_3()
                        .py_2()
                        .text_xl()
                        .font_family("Consolas")
                        .child("PS5"),
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
                .child(Self::button(
                    0,
                    "Conectar ao PS5 pareado",
                    Action::Connect,
                    cx,
                ))
                .child(Self::button(2, "Acordar PS5 pareado", Action::Wake, cx));
        } else {
            body = body.child(Self::button(
                0,
                "Parear console",
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
            .child("Bitrate solicitado");
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
        for (path, label) in [("nr", "DLSS NR"), ("sr", "Super-resolução")] {
            if let Some(i) = self.fields.iter().position(|f| f.path == path) {
                let enabled = self.values[i] != 0.;
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
                            Action::Change(i, if enabled { -1. } else { 1. }),
                            cx,
                        )),
                );
            }
        }
        Self::card("Streaming", body)
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
