mod metrics;
mod settings;
mod updater;
use gpui_kit::component::{button::*, tooltip::Tooltip, *};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use metrics::{Snapshot, gb, rate};
use settings::Settings;
use std::{sync::mpsc, time::Duration};

struct Monitor {
    data: Snapshot,
    displayed_usage: Vec<f32>,

    receiver: mpsc::Receiver<Snapshot>,
    settings: Settings,
    controls: bool,
    by_memory: bool,
    selected: Option<String>,
    error: Option<String>,
    update_receiver: mpsc::Receiver<(bool, Result<Option<String>, String>)>,
    update_request: mpsc::SyncSender<()>,
    update_message: Option<String>,
    update_version: Option<String>,
}
impl Monitor {
    fn new(cx: &mut Context<Self>) -> Self {
        let settings = Settings::load();
        let (update_sender, update_receiver) = mpsc::channel();
        let (update_request, requests) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut timeout = updater::STARTUP_DELAY;
            loop {
                let manual = match requests.recv_timeout(timeout) {
                    Ok(()) => true,
                    Err(mpsc::RecvTimeoutError::Timeout) => false,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                };
                timeout = updater::CHECK_INTERVAL;
                if !manual && cfg!(debug_assertions) {
                    continue;
                }
                if update_sender.send((manual, updater::check())).is_err() {
                    break;
                }
            }
        });
        let interval = settings.interval.clamp(1, 10);
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut collector = metrics::Collector::new();
            loop {
                std::thread::sleep(Duration::from_secs(interval));
                if sender.send(collector.sample()).is_err() {
                    break;
                }
            }
        });
        cx.spawn(async |view, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(33))
                    .await;
                if view
                    .update(cx, |this, cx| {
                        if let Ok((manual, result)) = this.update_receiver.try_recv() {
                            match result {
                                Ok(Some(version)) => {
                                    this.update_message = Some(format!(
                                        "Version {version} available · manual download"
                                    ));
                                    this.update_version = Some(version);
                                }
                                Ok(None) => {
                                    this.update_message = Some("Up to date".into());
                                    this.update_version = None;
                                }
                                Err(error) => {
                                    if manual {
                                        this.update_message =
                                            Some(format!("Update check failed: {error}"));
                                    }
                                }
                            }
                            cx.notify();
                        }
                        let mut changed = false;
                        for (usage, core) in this.displayed_usage.iter_mut().zip(&this.data.cores) {
                            let delta = core.usage - *usage;
                            if delta.abs() > 0.05 {
                                *usage += if this.settings.motion {
                                    delta * 0.14
                                } else {
                                    delta
                                };
                                changed = true;
                            }
                        }
                        if changed {
                            cx.notify();
                        }
                        if let Ok(data) = this.receiver.try_recv() {
                            if this.displayed_usage.len() != data.cores.len() {
                                this.displayed_usage = data.cores.iter().map(|c| c.usage).collect();
                            }
                            this.data = data;
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        Self {
            data: Snapshot::default(),
            displayed_usage: Vec::new(),

            receiver,
            settings,
            controls: false,
            by_memory: false,
            selected: None,
            error: None,
            update_receiver,
            update_request,
            update_message: None,
            update_version: None,
        }
    }
    fn save(&mut self) {
        self.error = self.settings.save().err();
    }
    fn accent(&self) -> u32 {
        [0x5b9bff, 0xf3f4f6, 0xff8468][self.settings.palette % 3]
    }
    fn muted(&self) -> u32 {
        if self.settings.light {
            0x69717f
        } else {
            0x9c9ca7
        }
    }
    fn track(&self) -> u32 {
        if self.settings.light {
            0xd8dde4
        } else {
            0x30313f
        }
    }
    fn badge(&self, glyph: &str, color: u32) -> Div {
        div()
            .flex()
            .items_center()
            .justify_center()
            .size(px(20.))
            .rounded_full()
            .bg(rgb(self.track()))
            .text_color(rgb(color))
            .text_size(px(12.))
            .child(Icon::default().data(format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><g fill="none" stroke="white" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">{}</g></svg>"#, match glyph {
                "▦" | "◌" => "<rect x='6' y='6' width='12' height='12' rx='2'/><path d='M9 2v4m6-4v4M9 18v4m6-4v4M2 9h4m-4 6h4m12-6h4m-4 6h4'/>",
                "▥" => "<rect x='3' y='7' width='18' height='10' rx='2'/><path d='M7 10v4m5-4v4m5-4v4M7 17v3m5-3v3m5-3v3'/>",
                "↓" => "<path d='M12 3v17m-6-6 6 6 6-6'/>",
                "↑" => "<path d='M12 21V4m-6 6 6-6 6 6'/>",
                "ϟ" => "<path d='m14 2-9 12h6l-1 8 9-12h-6z'/>",
                "♧" => "<path d='M9 14V5a3 3 0 0 1 6 0v9a5 5 0 1 1-6 0zM12 8v9'/>",
                _ => "<path d='m12 3 9 5-9 5-9-5zm-9 9 9 5 9-5m-18 5 9 5 9-5'/>",
            }).as_bytes()).size(px(11.)))
    }
    fn gauge_badge(&self, glyph: &str, color: u32, progress: f32) -> Div {
        let fraction = (progress / 100.).clamp(0.001, 0.9999);
        let angle = fraction * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        let (x, y) = (10. + 9. * angle.cos(), 10. + 9. * angle.sin());
        let arc = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path d="M 10 1 A 9 9 0 {} 1 {x} {y}" fill="none" stroke="white" stroke-width="2" stroke-linecap="round"/></svg>"#,
            if fraction > 0.5 { 1 } else { 0 }
        );
        div()
            .relative()
            .size(px(20.))
            .child(self.badge(glyph, color))
            .child(
                Icon::default()
                    .data(arc.as_bytes())
                    .size(px(20.))
                    .absolute()
                    .inset_0()
                    .text_color(rgb(color)),
            )
    }
    fn figure(&self, glyph: &str, value: String, color: u32) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(7.))
            .child(match glyph {
                "▦" => self.gauge_badge(glyph, color, self.data.cpu),
                "▥" => self.gauge_badge(
                    glyph,
                    color,
                    100. * self.data.memory as f32 / self.data.total_memory.max(1) as f32,
                ),
                _ => self.badge(glyph, color),
            })
            .child(value)
    }
    fn process_list(&self, rows: usize, cx: &mut Context<Self>) -> Div {
        let mut list = self.data.processes.clone();
        if self.by_memory {
            list.sort_by(|a, b| b.memory.cmp(&a.memory));
        }
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .flex_1()
            .min_w(px(170.))
            .child(
                div()
                    .flex()
                    .gap(px(6.))
                    .child(
                        Button::new("cpu-sort")
                            .label("CPU")
                            .selected(!self.by_memory)
                            .small()
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.by_memory = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("mem-sort")
                            .label("Memory")
                            .selected(self.by_memory)
                            .small()
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.by_memory = true;
                                cx.notify();
                            })),
                    ),
            )
            .children(list.iter().take(rows).enumerate().map(|(i, p)| {
                let name = p.name.clone();
                let keyboard_name = name.clone();
                div()
                    .id(("process", i))
                    .tab_index(0)
                    .focusable()
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                        if event.keystroke.key == "enter" || event.keystroke.key == "space" {
                            this.selected = if this.selected.as_ref() == Some(&keyboard_name) {
                                None
                            } else {
                                Some(keyboard_name.clone())
                            };
                            cx.stop_propagation();
                            cx.notify();
                        }
                    }))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .h(px(22.))
                    .cursor_pointer()
                    .rounded(px(6.))
                    .hover(|s| s.bg(rgb(self.track())))
                    .child(self.gauge_badge("◌", self.accent(), p.cpu))
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(p.name.clone()),
                    )
                    .child(if self.by_memory {
                        format!("{} GB", gb(p.memory))
                    } else {
                        format!("{:.1}%", p.cpu)
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected = if this.selected.as_ref() == Some(&name) {
                            None
                        } else {
                            Some(name.clone())
                        };
                        cx.notify();
                    }))
            }))
    }
}
impl Render for Monitor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut size = window.viewport_size();
        if let Some(inset) = window.client_inset() {
            size.width -= inset * 2.;
            size.height -= inset * 2.;
        }
        let wide = size.width > px(680.);
        let compact = size.height < px(250.);
        let muted = self.muted();
        let accent = self.accent();
        let memory_fraction = self.data.memory as f32 / self.data.total_memory.max(1) as f32;
        let core_height = if wide {
            (f32::from(size.height) - 245.).clamp(60., 340.)
        } else {
            (f32::from(size.height) - 360.).clamp(50., 160.)
        };
        let core_count = self.data.cores.len().max(1) as f32;
        let available = (f32::from(size.width) * if wide { 0.52 } else { 1.0 } - 30.).max(1.);
        let core_gap = (available / core_count * 0.14).min(4.);
        let core_width = (available / core_count - core_gap).clamp(0.1, 25.);
        let mut panel = div()
            .flex()
            .flex_col()
            .w_full()
            .min_h_full()
            .p(px(15.))
            .gap(px(12.))
            .rounded(px(18.))
            .bg(if self.settings.light {
                rgba(0xf1f4f7ed)
            } else {
                rgba(0x181923ed)
            })
            .text_color(rgb(if self.settings.light {
                0x182130
            } else {
                0xe4e5ec
            }))
            .text_size(px(12.));
        panel = panel.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    self.figure("▦", format!("{:.0} %", self.data.cpu), accent)
                        .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
                        .text_size(px(17.))
                        .font_weight(FontWeight::SEMIBOLD),
                )
                .child(
                    Button::new("settings")
                        .label(if self.controls { "Done" } else { "•••" })
                        .small()
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.controls = !this.controls;
                            cx.notify();
                        })),
                ),
        );
        if self.controls {
            panel = panel.child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        Button::new("hide")
                            .label("Minimize")
                            .small()
                            .ghost()
                            .on_click(|_, window, _| window.minimize_window()),
                    )
                    .child(
                        Button::new("quit")
                            .label("Quit")
                            .small()
                            .ghost()
                            .on_click(|_, _, cx| cx.quit()),
                    ),
            );
            panel = panel.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .children(
                        [
                            ("Cores", self.settings.cores),
                            ("Memory", self.settings.memory),
                            ("Network", self.settings.network),
                            ("Processes", self.settings.processes),
                            ("Light", self.settings.light),
                            ("Motion", self.settings.motion),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(i, (label, on))| {
                            Button::new(("toggle", i))
                                .label(format!("{} {}", if on { "●" } else { "○" }, label))
                                .selected(on)
                                .small()
                                .ghost()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    match i {
                                        0 => this.settings.cores = !this.settings.cores,
                                        1 => this.settings.memory = !this.settings.memory,
                                        2 => this.settings.network = !this.settings.network,
                                        3 => this.settings.processes = !this.settings.processes,
                                        4 => this.settings.light = !this.settings.light,
                                        _ => this.settings.motion = !this.settings.motion,
                                    };
                                    this.save();
                                    Theme::change(
                                        if this.settings.light {
                                            ThemeMode::Light
                                        } else {
                                            ThemeMode::Dark
                                        },
                                        None,
                                        cx,
                                    );
                                    cx.notify();
                                }))
                        }),
                    )
                    .child(
                        Button::new("palette")
                            .label("Sea / Graphite / Warm")
                            .small()
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.settings.palette = (this.settings.palette + 1) % 3;
                                this.save();
                                cx.notify();
                            })),
                    ),
            );
        }
        let mut readings = div().flex().flex_col().gap(px(10.)).flex_1().min_w(px(0.));
        if self.settings.cores && !compact {
            readings = readings.child(div().flex().items_end().gap(px(core_gap)).children(
                self.data.cores.iter().enumerate().map(|(i, c)| {
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(3.))
                        .child(
                            div()
                                .relative()
                                .w(px(core_width))
                                .h(px(core_height))
                                .rounded(px(8.))
                                .bg(rgb(self.track()))
                                .child(
                                    div()
                                        .absolute()
                                        .bottom_0()
                                        .w_full()
                                        .h(px((core_height
                                            * self
                                                .displayed_usage
                                                .get(i)
                                                .copied()
                                                .unwrap_or(c.usage)
                                            / 100.)
                                            .max(4.)))
                                        .rounded(px(8.))
                                        .bg(rgb(accent)),
                                )
                                .child(
                                    div()
                                        .absolute()
                                        .left(px(core_width * 0.2))
                                        .bottom(px(core_height
                                            * (c.mhz as f32
                                                / self
                                                    .data
                                                    .cores
                                                    .iter()
                                                    .map(|c| c.mhz)
                                                    .max()
                                                    .unwrap_or(1)
                                                    .max(1)
                                                    as f32)
                                                .min(1.)))
                                        .w(px(core_width * 0.6))
                                        .h(px(1.5))
                                        .rounded_full()
                                        .bg(rgb(muted)),
                                ),
                        )
                        .when(core_width >= 17., |el| {
                            el.child(div().text_size(px(10.)).child(format!("{:.0}", c.usage)))
                                .child(div().text_size(px(8.5)).text_color(rgb(muted)).child(
                                    if c.mhz == 0 {
                                        "—".into()
                                    } else {
                                        format!("{:.1}", c.mhz as f32 / 1000.)
                                    },
                                ))
                        })
                        .id(("core", i))
                        .tooltip({
                            let detail = format!(
                                "Core {} · {:.1}% · reported {} MHz",
                                i + 1,
                                c.usage,
                                c.mhz
                            );
                            move |window, cx| Tooltip::new(detail.clone()).build(window, cx)
                        })
                }),
            ));
        }
        if self.settings.memory {
            readings = readings
                .child(
                    self.figure(
                        "▥",
                        format!(
                            "{} / {} GB",
                            gb(self.data.memory),
                            gb(self.data.total_memory)
                        ),
                        0x4dcc7c,
                    )
                    .text_size(px(17.)),
                )
                .child(
                    div()
                        .h(px(5.))
                        .w_full()
                        .rounded_full()
                        .bg(rgb(self.track()))
                        .child(
                            div()
                                .h_full()
                                .w(relative(memory_fraction.clamp(0., 1.)))
                                .rounded_full()
                                .bg(rgb(0x4dcc7c)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .text_color(rgb(muted))
                        .text_size(px(10.5))
                        .child("Compressed —")
                        .child(format!(
                            "Swap {} / {} GB",
                            gb(self.data.swap),
                            gb(self.data.total_swap)
                        )),
                );
        }
        if self.settings.network {
            readings = readings.child(
                div()
                    .flex()
                    .justify_between()
                    .child(self.figure("↓", rate(self.data.download), muted))
                    .child(self.figure("↑", rate(self.data.upload), muted)),
            );
        }
        if !compact {
            let sensor = self
                .data
                .temperature
                .as_ref()
                .map(|(label, _)| label.clone())
                .unwrap_or_else(|| "Temperature unavailable".into());
            readings = readings.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .justify_between()
                    .text_color(rgb(muted))
                    .child(self.figure("▧", "GPU —".into(), 0xb294ff))
                    .child(self.figure("ϟ", "— W".into(), muted))
                    .child(
                        div()
                            .id("temperature")
                            .child(
                                self.figure(
                                    "♧",
                                    self.data
                                        .temperature
                                        .as_ref()
                                        .map(|(_, t)| format!("{t:.0}°"))
                                        .unwrap_or_else(|| "— °C".into()),
                                    muted,
                                ),
                            )
                            .tooltip(move |window, cx| {
                                Tooltip::new(sensor.clone()).build(window, cx)
                            }),
                    ),
            );
        }
        let available_rows = if wide {
            f32::from(size.height) - 100.
        } else {
            f32::from(size.height)
                - 100.
                - if self.settings.cores && !compact {
                    core_height + if core_width >= 17. { 40. } else { 10. }
                } else {
                    0.
                }
                - if self.settings.memory { 68. } else { 0. }
                - if self.settings.network { 30. } else { 0. }
                - if !compact { 56. } else { 0. }
        };
        let rows = (available_rows / 30.).floor().max(0.) as usize;
        let body = div()
            .flex()
            .gap(px(22.))
            .when(!wide, |el| el.flex_col())
            .child(readings)
            .when(self.settings.processes && !compact && rows > 0, |el| {
                el.child(self.process_list(if wide { rows } else { rows.min(5) }, cx))
            });
        panel = panel.child(body);
        if self.data.cores.is_empty() {
            panel = panel.child(
                div()
                    .text_color(rgb(muted))
                    .child("Reading system metrics…"),
            );
        }
        if let Some(name) = &self.selected {
            if let Some(p) = self.data.processes.iter().find(|p| &p.name == name) {
                panel = panel.child(
                    div()
                        .p(px(10.))
                        .rounded(px(10.))
                        .bg(rgb(self.track()))
                        .child(format!(
                            "{}   {:.1}% CPU   {} GB   {} processes",
                            p.name,
                            p.cpu,
                            gb(p.memory),
                            p.pids.len()
                        )),
                );
            }
        }
        if self.controls {
            panel = panel.child(
                Button::new("check-update")
                    .label("Check for updates")
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        let _ = this.update_request.try_send(());
                        this.update_message = Some("Checking…".into());
                        cx.notify();
                    })),
            );

            panel = panel.child(
                div().text_size(px(10.5)).text_color(rgb(muted)).child(
                    self.update_message
                        .clone()
                        .unwrap_or_else(|| "Updates: manual download · checks daily".into()),
                ),
            );
            if let Some(version) = &self.update_version {
                if let Ok(url) = updater::release_page(version) {
                    panel = panel.child(
                        Button::new("download-update")
                            .label("Open release")
                            .small()
                            .ghost()
                            .on_click(move |_, _, cx| cx.open_url(&url)),
                    );
                }
            }
        }
        if let Some(error) = &self.error {
            panel = panel.child(format!("Could not save settings: {error}"));
        }
        div()
            .id("panel-scroll")
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.selected = None;
                    this.controls = false;
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, _, cx| {
                    this.controls = !this.controls;
                    cx.notify();
                }),
            )
            .size_full()
            .overflow_y_scroll()
            .child(panel)
    }
}
fn main() {
    if std::env::args().any(|a| a == "--version") {
        println!("ftop {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            Theme::change(
                if Settings::load().light {
                    ThemeMode::Light
                } else {
                    ThemeMode::Dark
                },
                None,
                cx,
            );
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(760.), px(430.)),
                        cx,
                    ))),
                    window_min_size: Some(size(px(280.), px(180.))),
                    titlebar: None,
                    window_decorations: Some(WindowDecorations::Client),
                    window_background: WindowBackgroundAppearance::Blurred,
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    window.set_window_title("ftop");
                    cx.new(Monitor::new)
                },
            )
            .expect("Unable to open ftop window");
            cx.activate(true);
        });
}
