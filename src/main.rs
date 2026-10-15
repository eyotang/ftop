#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod chrome;
mod i18n;
mod metrics;
mod sampling_gate;
mod settings;
mod updater;
use chrome::PanelButton;
use gpui_kit::base::Button;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use metrics::{Snapshot, gb, rate};
use settings::Settings;
use std::{sync::mpsc, time::Duration};

struct Monitor {
    data: Snapshot,
    displayed_usage: Vec<f32>,

    _visibility: Subscription,
    settings: Settings,
    controls: bool,
    install_guide: bool,
    by_memory: bool,
    selected: Option<String>,
    error: Option<String>,

    update_request: mpsc::SyncSender<()>,
    updates: updater::Status,
}
impl Monitor {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut settings = Settings::load();
        if std::env::args().any(|arg| arg == "--no-motion") {
            settings.motion = false;
        }
        if std::env::args().any(|arg| arg == "--motion") {
            settings.motion = true;
        }
        let (update_sender, update_receiver) = async_channel::bounded(1);
        let (update_request, requests) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("ftop-updater".into())
            .spawn(move || {
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
                    if update_sender
                        .send_blocking(updater::Event::Checking)
                        .is_err()
                    {
                        break;
                    }
                    let result = updater::check();
                    if update_sender
                        .send_blocking(updater::Event::Finished { manual, result })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .expect("Unable to start update worker");
        if std::env::args().any(|arg| arg == "--benchmark-hidden") {
            cx.spawn_in(window, async |_, cx| {
                cx.background_executor().timer(Duration::from_secs(3)).await;
                let _ = cx.update(|window, _| window.minimize_window());
            })
            .detach();
        }
        let interval = settings.interval.clamp(1, 10);
        let visible = sampling_gate::SamplingGate::new(window);
        let worker_visibility = visible.clone();
        let visibility = cx.observe_window_visibility(window, move |_, visibility, _, _| {
            visible.set_visible(visibility.is_visible())
        });
        let (sender, receiver) = async_channel::bounded(1);
        std::thread::Builder::new()
            .name("ftop-collector".into())
            .spawn(move || {
                let mut collector = metrics::Collector::new();
                loop {
                    std::thread::sleep(Duration::from_secs(interval));
                    worker_visibility.wait();
                    if sender.send_blocking(collector.sample()).is_err() {
                        break;
                    }
                }
            })
            .expect("Unable to start sampling worker");
        cx.spawn(async move |view, cx| {
            while let Ok(data) = receiver.recv().await {
                let Ok(animate) = view.update(cx, |this, cx| {
                    let animate =
                        this.settings.motion && this.displayed_usage.len() == data.cores.len();
                    if !animate {
                        this.displayed_usage = data.cores.iter().map(|c| c.usage).collect();
                    }
                    this.data = data;
                    cx.notify();
                    animate
                }) else {
                    break;
                };
                // Optional, bounded transition: no animation task runs between samples.
                if animate {
                    for frame in 1..=6 {
                        cx.background_executor()
                            .timer(Duration::from_millis(16))
                            .await;
                        if view
                            .update(cx, |this, cx| {
                                for (usage, core) in
                                    this.displayed_usage.iter_mut().zip(&this.data.cores)
                                {
                                    *usage = if frame == 6 {
                                        core.usage
                                    } else {
                                        *usage + (core.usage - *usage) * 0.5
                                    };
                                }
                                cx.notify();
                            })
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            }
        })
        .detach();
        cx.spawn(async move |view, cx| {
            while let Ok(event) = update_receiver.recv().await {
                if view
                    .update(cx, |this, cx| {
                        this.updates.apply(event);
                        if this.controls {
                            this.updates.dismiss();
                            this.install_guide |= this.updates.version.is_some();
                        }
                        cx.notify();
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

            _visibility: visibility,
            settings,
            controls: false,
            install_guide: false,
            by_memory: false,
            selected: None,
            error: None,

            update_request,
            updates: updater::Status::default(),
        }
    }
    fn save(&mut self) {
        self.error = self.settings.save().err();
    }
    fn window_controls(&self, cx: &mut Context<Self>, settings_page: bool) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(3.))
            .when(!settings_page, |row| {
                row.child(
                    Button::new("settings")
                        .label("…")
                        .accessibility_label(self.t("Settings"))
                        .small()
                        .p(px(0.))
                        .size(px(22.))
                        .selected(self.updates.visible_version().is_some())
                        .tooltip({
                            let text = self.t(if self.updates.visible_version().is_some() {
                                "Update available · open Settings"
                            } else {
                                "Settings"
                            });
                            move |_, cx| chrome::tooltip(text.clone(), cx)
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.controls = true;
                            this.install_guide |= this.updates.version.is_some();
                            this.updates.dismiss();
                            cx.notify();
                        })),
                )
            })
            .child(
                Button::new("window-minimize")
                    .label("−")
                    .accessibility_label(self.t("Minimize"))
                    .small()
                    .p(px(0.))
                    .size(px(22.))
                    .tooltip({
                        let text = self.t("Minimize");
                        move |_, cx| chrome::tooltip(text.clone(), cx)
                    })
                    .on_click(|_, window, _| window.minimize_window()),
            )
            .child(
                Button::new("window-quit")
                    .label("×")
                    .accessibility_label(self.t("Quit"))
                    .small()
                    .p(px(0.))
                    .size(px(22.))
                    .text_color(rgb(0xff6970))
                    .tooltip({
                        let text = self.t("Quit");
                        move |_, cx| chrome::tooltip(text.clone(), cx)
                    })
                    .on_click(|_, _, cx| cx.quit()),
            )
    }
    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        if event.keystroke.modifiers.secondary() {
            match key {
                "q" => cx.quit(),
                "m" => window.minimize_window(),
                "u" => {
                    self.controls = true;
                    self.check_updates();
                    cx.notify();
                }
                "," => {
                    self.controls = !self.controls;
                    self.install_guide |= self.updates.version.is_some();
                    self.updates.dismiss();
                    cx.notify();
                }
                _ => return,
            }
            cx.stop_propagation();
        } else if key == "escape" {
            self.selected = None;
            self.controls = false;
            self.install_guide = false;
            cx.notify();
        }
    }
    fn settings_page(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let mut panel = div()
            .flex()
            .flex_col()
            .gap(px(15.))
            .p(px(15.))
            .child(self.update_center(cx));
        panel = panel.child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(6.))
                .child(self.t("Language"))
                .children(
                    i18n::Language::ALL
                        .into_iter()
                        .enumerate()
                        .map(|(i, language)| {
                            Button::new(("language", i))
                                .label(self.t(language.label()))
                                .small()
                                .selected(self.settings.language == language)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.settings.language = language;
                                    this.save();
                                    cx.notify();
                                }))
                        }),
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
                            .label(format!("{} {}", if on { "●" } else { "○" }, self.t(label)))
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
                                cx.notify();
                            }))
                    }),
                )
                .child(
                    Button::new("palette")
                        .label(self.t("Sea / Graphite / Warm"))
                        .small()
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.settings.palette = (this.settings.palette + 1) % 3;
                            this.save();
                            cx.notify();
                        })),
                ),
        );
        let mut status = self
            .data
            .capabilities
            .iter()
            .map(|(name, detail)| self.t(&format!("{name}: {detail}")))
            .collect::<Vec<_>>();
        status.push(
            self.data
                .gpu_info
                .clone()
                .unwrap_or_else(|| "GPU 等待采样".into()),
        );
        let status = self.t(&status.join("\n"));
        panel = panel.child(
            Button::new("collection-status")
                .label(self.t("Collection status"))
                .small()
                .tooltip(move |_, cx| chrome::tooltip(status.clone(), cx)),
        );

        if let Some(error) = &self.error {
            panel = panel.child(format!("{} {error}", self.t("Could not save settings:")));
        }
        div()
            .id("settings-page")
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(chrome::RADIUS - 1.))
            .bg(if self.settings.light {
                rgba(0xf1f4f7ed)
            } else {
                rgba(0x181923ed)
            })
            .font_family(chrome::FONT)
            .text_size(px(12.))
            .text_color(rgb(if self.settings.light {
                0x182130
            } else {
                0xe4e5ec
            }))
            .on_key_down(cx.listener(Self::key_down))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(15.))
                    .py(px(13.))
                    .child(
                        Button::new("settings-back")
                            .label(format!("‹ {}", self.t("Back")))
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.controls = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(17.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.t("Settings"))
                            .on_mouse_down(MouseButton::Left, |_, window, _| {
                                window.start_window_move()
                            }),
                    )
                    .child(self.window_controls(cx, true)),
            )
            .child(
                div()
                    .id("settings-scroll")
                    .flex_1()
                    .min_h(px(0.))
                    .w_full()
                    .overflow_y_scroll()
                    .child(panel),
            )
    }
    fn check_updates(&mut self) {
        if !self.updates.begin_manual() {
            return;
        }
        if self.update_request.try_send(()).is_err() {
            self.updates.apply(updater::Event::Finished {
                manual: true,
                result: Err("Update service unavailable".into()),
            });
        }
    }
    fn update_center(&self, cx: &mut Context<Self>) -> Div {
        let version = self
            .updates
            .version
            .as_deref()
            .unwrap_or(env!("CARGO_PKG_VERSION"));
        let mut center =
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .p(px(10.))
                .rounded(px(10.))
                .bg(rgba(0x5b9bff18))
                .border_1()
                .border_color(rgba(0x5b9bff44))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(self.t("Updates")),
                )
                .child(format!("ftop {}", env!("CARGO_PKG_VERSION")))
                .child(div().text_size(px(10.5)).child(
                    self.t("Automatic checks: 20 seconds after launch, then every 24 hours."),
                ))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(6.))
                        .child(
                            Button::new("check-update")
                                .label(self.t(if self.updates.checking {
                                    "Checking…"
                                } else {
                                    "Check for updates"
                                }))
                                .small()
                                .disabled(self.updates.checking)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.check_updates();
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("show-install-guide")
                                .label(self.t("Installation guide"))
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.install_guide = !this.install_guide;
                                    cx.notify();
                                })),
                        ),
                )
                .when_some(self.updates.message.as_ref(), |center, message| {
                    center.child(div().text_size(px(10.5)).child(self.t(message)))
                });
        if self.install_guide {
            let steps: &[&str] = if cfg!(target_os = "macos") {
                &[
                    "1. Download the DMG for this Mac.",
                    "2. Quit ftop using the top-right close button.",
                    "3. Open the DMG, drag ftop.app to Applications, choose Replace, then reopen ftop.",
                ]
            } else if cfg!(target_os = "windows") {
                &[
                    "1. Download the Windows portable ZIP.",
                    "2. Quit ftop using the top-right close button.",
                    "3. Extract the ZIP, replace the old files, then launch ftop.exe.",
                ]
            } else {
                &[
                    "1. Download the Ubuntu deb package.",
                    "2. Quit ftop using the top-right close button.",
                    "3. Install the deb package, then reopen ftop.",
                ]
            };
            center = center
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(self.t("Installation guide")),
                )
                .child(self.t("This package currently requires manual installation."));
            if let Ok(url) =
                updater::package_url(version, std::env::consts::OS, std::env::consts::ARCH)
            {
                center = center
                    .child(
                        Button::new("download-platform-package")
                            .label(self.t(if self.updates.version.is_some() {
                                "Download update"
                            } else {
                                "Download current version"
                            }))
                            .small()
                            .bg(rgba(0x5b9bff30))
                            .on_click(move |_, _, cx| cx.open_url(&url)),
                    )
                    .children(
                        steps
                            .iter()
                            .map(|step| div().text_size(px(11.)).child(self.t(step))),
                    );
                if cfg!(target_os = "linux") {
                    let command = format!("sudo apt install ./ftop-{version}-linux-x86_64.deb");
                    let copy = command.clone();
                    center = center
                        .child(div().text_size(px(10.5)).child(command))
                        .child(
                            Button::new("copy-install-command")
                                .label(self.t("Copy install command"))
                                .small()
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()))
                                }),
                        );
                }
            } else {
                center = center.child(self.t("No package for this platform"));
            }
            if let Ok(url) = updater::release_page(version) {
                center = center.child(
                    Button::new("release-details")
                        .label(self.t("Release notes"))
                        .small()
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                );
            }
        }
        center
    }
    fn t(&self, text: &str) -> String {
        self.settings.language.text(text)
    }
    fn gpu_color(&self) -> u32 {
        if self.settings.light {
            [0x7a4fd6, 0x8a6d2f, 0xb83280][self.settings.palette % 3]
        } else {
            [0xb294ff, 0xc9b98f, 0xff8fc4][self.settings.palette % 3]
        }
    }
    fn accent(&self) -> u32 {
        if self.settings.light {
            [0x1f5fe0, 0x1f2937, 0xdc4a2b][self.settings.palette % 3]
        } else {
            [0x5b9bff, 0xf3f4f6, 0xff8468][self.settings.palette % 3]
        }
    }
    fn core_color(&self, group: usize) -> u32 {
        let last = self
            .data
            .cores
            .iter()
            .map(|c| c.group)
            .max()
            .unwrap_or(0)
            .max(1);
        let start = self.accent();
        let end = if self.settings.light {
            [0x0b8f7e, 0x6b7280, 0xb9770a][self.settings.palette % 3]
        } else {
            [0x45d1bd, 0x9aa1ad, 0xffc857][self.settings.palette % 3]
        };
        let t = group as f32 / last as f32;
        [16, 8, 0].into_iter().fold(0, |color, shift| {
            let a = ((start >> shift) & 255) as f32;
            let b = ((end >> shift) & 255) as f32;
            color | (((a + (b - a) * t) as u32) << shift)
        })
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
    fn metric_description(&self, glyph: &str) -> String {
        self.t(&match glyph {
            "▦" => format!("CPU · 处理器总占用 {:.0}%", self.data.cpu),
            "◌" => "CPU · 进程处理器占用".into(),
            "▥" => format!(
                "内存 · 已用 {} / 总计 {} GB · {}",
                gb(self.data.memory),
                gb(self.data.total_memory),
                self.data
                    .memory_pressure_info
                    .as_deref()
                    .unwrap_or("压力传感器未开放")
            ),
            "▧" => format!(
                "GPU · 图形处理器占用 {} · GPU 内存 {} · 温度 {} · 频率 {} · {}",
                self.data
                    .gpu_usage
                    .map(|v| format!("{v:.0}%"))
                    .unwrap_or_else(|| "不可用".into()),
                self.data
                    .gpu_memory
                    .map(|v| format!("{} GB", gb(v)))
                    .unwrap_or_else(|| "不可用".into()),
                self.data
                    .gpu_temperature
                    .map(|v| format!("{v:.0}°C"))
                    .unwrap_or_else(|| "当前接口无读数".into()),
                self.data
                    .gpu_frequency
                    .map(|(current, maximum)| format!("{current}/{maximum} MHz"))
                    .unwrap_or_else(|| "当前接口无读数".into()),
                self.data.gpu_info.as_deref().unwrap_or("等待 GPU 采样")
            ),
            "↓" => "网络 · 下载速率".into(),
            "↑" => "网络 · 上传速率".into(),
            "ϟ" => format!(
                "GPU 功耗（W）· {}",
                self.data
                    .gpu_info
                    .as_deref()
                    .unwrap_or("当前接口无读数功率传感器")
            ),
            "⌂" => {
                self.data.power_label.clone().unwrap_or_else(|| {
                    "当前接口未获取到功率（不以 CPU/GPU 功率冒充整机功率）".into()
                })
            }
            "♧" => self
                .data
                .temperature
                .as_ref()
                .map(|(label, _)| format!("温度 · {label}（°C）"))
                .unwrap_or_else(|| "系统/当前接口无读数温度传感器".into()),
            _ => "系统指标".into(),
        })
    }
    fn badge(&self, glyph: &str, color: u32) -> Div {
        let description = self.metric_description(glyph);
        self.badge_with_description(glyph, color, description)
    }
    fn badge_with_description(&self, glyph: &str, color: u32, description: String) -> Div {
        div()
            .flex()
            .items_center()
            .justify_center()
            .size(px(20.))
            .rounded_full()
            .bg(rgb(self.track()))
            .text_color(rgb(color))
            .text_size(px(12.))
            .child(div().id(SharedString::from(format!("badge-{glyph}"))).size(px(20.)).flex().items_center().justify_center().tooltip(move |_, cx| chrome::tooltip(description.clone(), cx)).child(svg().data(format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><g fill="none" stroke="white" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">{}</g></svg>"#, match glyph {
                "▦" | "◌" => "<rect x='6' y='6' width='12' height='12' rx='2'/><path d='M9 2v4m6-4v4M9 18v4m6-4v4M2 9h4m-4 6h4m12-6h4m-4 6h4'/>",
                "▧" => "<rect x='2' y='6' width='19' height='12' rx='2'/><circle cx='10' cy='12' r='3'/><path d='M16 10h2m-2 4h2M6 18v3m4-3v3m4-3v3M2 4v16'/>",
                "▥" => "<rect x='3' y='7' width='18' height='10' rx='2'/><path d='M7 10v4m5-4v4m5-4v4M7 17v3m5-3v3m5-3v3'/>",
                "↓" => "<path d='M12 3v17m-6-6 6 6 6-6'/>",
                "↑" => "<path d='M12 21V4m-6 6 6-6 6 6'/>",
                "ϟ" => "<path d='m14 2-9 12h6l-1 8 9-12h-6z'/>",
                "⌂" => "<path d='M5 6h14v11H5zM2 20h20'/>",
                "♧" => "<path d='M9 14V5a3 3 0 0 1 6 0v9a5 5 0 1 1-6 0zM12 8v9'/>",
                _ => "<path d='m12 3 9 5-9 5-9-5zm-9 9 9 5 9-5m-18 5 9 5 9-5'/>",
            }).as_bytes()).size(px(11.)).text_color(rgb(color))))
    }
    fn bar(
        &self,
        usage: f32,
        frequency: (u64, u64),
        width: f32,
        height: f32,
        color: u32,
    ) -> impl IntoElement {
        let track = self.track();
        let muted = self.muted();
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let radius = px((width / 2.).min(8.));
                window.paint_quad(fill(bounds, rgb(track)).corner_radii(radius));
                let filled = px((height * usage.clamp(0., 100.) / 100.).max(4.).min(height));
                let area = Bounds::new(
                    point(bounds.left(), bounds.bottom() - filled),
                    size(bounds.size.width, filled),
                );
                window.paint_quad(fill(area, rgb(color)).corner_radii(radius));
                if frequency.0 > 0 && frequency.1 > 0 {
                    let top = bounds.bottom()
                        - px(height * (frequency.0 as f32 / frequency.1 as f32).min(1.));
                    let tick = Bounds::new(
                        point(bounds.left() + px(width * 0.2), top),
                        size(px(width * 0.6), px(1.5)),
                    );
                    window.paint_quad(fill(tick, rgb(muted)).corner_radii(px(0.75)));
                }
            },
        )
        .w(px(width))
        .h(px(height))
    }
    fn gpu_column(&self, width: f32, height: f32, numbered: bool) -> Stateful<Div> {
        let frequency = self.data.gpu_frequency.unwrap_or((0, 0));
        div()
            .id("gpu-column")
            .tooltip({
                let detail = self.metric_description("▧");
                move |_, cx| chrome::tooltip(detail.clone(), cx)
            })
            .relative()
            .ml(px(10.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(3.))
            .child(self.bar(
                self.data.gpu_usage.unwrap_or(0.),
                frequency,
                width,
                height,
                self.gpu_color(),
            ))
            .when(numbered, |column| {
                // Match both CPU footer rows; the GPU figure lives in the shared header.
                column.child(div().text_size(px(10.)).child(" ")).child(
                    div()
                        .text_size(px(8.5))
                        .text_color(rgb(self.muted()))
                        .child(if frequency.0 > 0 {
                            format!("{:.1}", frequency.0 as f32 / 1000.)
                        } else {
                            "—".into()
                        }),
                )
            })
    }
    fn gauge_badge(&self, glyph: &str, color: u32, progress: f32) -> Div {
        let fraction = (progress / 100.).clamp(0., 1.);
        div()
            .relative()
            .size(px(20.))
            .child(self.badge(glyph, color))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        if fraction <= 0. {
                            return;
                        }
                        let mut path = PathBuilder::stroke(px(2.));
                        let center = bounds.center();
                        let segments = (fraction * 48.).ceil().max(1.) as usize;
                        for step in 0..=segments {
                            let angle = std::f32::consts::TAU * fraction * step as f32
                                / segments as f32
                                - std::f32::consts::FRAC_PI_2;
                            let position = point(
                                center.x + px(9. * angle.cos()),
                                center.y + px(9. * angle.sin()),
                            );
                            if step == 0 {
                                path.move_to(position);
                            } else {
                                path.line_to(position);
                            }
                        }
                        if let Ok(path) = path.build() {
                            window.paint_path(path, rgb(color));
                        }
                    },
                )
                .size(px(20.))
                .absolute()
                .inset_0(),
            )
    }
    fn figure(&self, glyph: &str, value: String, color: u32) -> Stateful<Div> {
        let value = self.t(&value);
        let (number, unit) = value
            .split_once(' ')
            .filter(|(number, _)| number.parse::<f64>().is_ok())
            .unwrap_or((&value, ""));
        let description = self.metric_description(glyph);
        div()
            .id(SharedString::from(format!("metric-{glyph}")))
            .tooltip(move |_, cx| chrome::tooltip(description.clone(), cx))
            .rounded(px(5.))
            .hover(|style| style.bg(rgba(0x88889922)))
            .flex()
            .items_center()
            .gap(px(6.))
            .child(match glyph {
                "▦" => self.gauge_badge(glyph, color, self.data.cpu),
                "▧" => self.gauge_badge(glyph, color, self.data.gpu_usage.unwrap_or(0.)),
                "▥" => self.gauge_badge(
                    glyph,
                    color,
                    100. * self.data.memory as f32 / self.data.total_memory.max(1) as f32,
                ),
                _ => self.badge(glyph, color),
            })
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(3.))
                    .child(number.to_owned())
                    .child(
                        div()
                            .text_size(px(10.5))
                            .font_weight(FontWeight::NORMAL)
                            .text_color(rgb(self.muted()))
                            .child(unit.to_owned()),
                    ),
            )
    }
    fn process_list(&self, rows: usize, cx: &mut Context<Self>) -> Div {
        let mut list: Vec<_> = self.data.processes.iter().collect();
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
                            .accessibility_label(self.t("Sort by CPU"))
                            .size(px(20.))
                            .rounded_full()
                            .child(
                                self.badge_with_description(
                                    "▦",
                                    if !self.by_memory {
                                        0x182130
                                    } else {
                                        self.muted()
                                    },
                                    self.t("Sort by CPU"),
                                )
                                .bg(rgb(if !self.by_memory {
                                    self.accent()
                                } else {
                                    self.track()
                                })),
                            )
                            .focus_visible(|style| style.border_1().border_color(rgb(0x5b9bff)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.by_memory = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("mem-sort")
                            .accessibility_label(self.t("Sort by memory"))
                            .size(px(20.))
                            .rounded_full()
                            .child(
                                self.badge_with_description(
                                    "▥",
                                    if self.by_memory {
                                        0x182130
                                    } else {
                                        self.muted()
                                    },
                                    self.t("Sort by memory"),
                                )
                                .bg(rgb(if self.by_memory {
                                    self.accent()
                                } else {
                                    self.track()
                                })),
                            )
                            .focus_visible(|style| style.border_1().border_color(rgb(0x5b9bff)))
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
        if self.controls {
            return chrome::frame(self.settings_page(cx), window);
        }
        let mut size = window.viewport_size();
        if let Some(inset) = window.client_inset() {
            size.width -= inset * 2.;
            size.height -= inset * 2.;
        }
        let wide = size.width > px(680.);
        let compact = size.height < px(250.);
        let muted = self.muted();
        let accent = self.accent();
        let memory_color = match self.data.pressure {
            Some(2) => {
                if self.settings.light {
                    0xb26a00
                } else {
                    0xf0b54a
                }
            }
            Some(4) => {
                if self.settings.light {
                    0xd23f31
                } else {
                    0xff6b5e
                }
            }
            _ => {
                if self.settings.light {
                    0x1c9a50
                } else {
                    0x4fd17f
                }
            }
        };
        let memory_fraction = self.data.memory as f32 / self.data.total_memory.max(1) as f32;
        let core_height = if wide {
            (f32::from(size.height) - 275.).clamp(60., 340.)
        } else {
            (f32::from(size.height) - 390.).clamp(50., 160.)
        };
        let core_count = self.data.cores.len().max(1) as f32;
        let gpu_width = if wide { 78. } else { 64. };
        let available = (f32::from(size.width) * if wide { 0.52 } else { 1.0 }
            - 30.
            - if self.data.gpu_usage.is_some() {
                gpu_width + 14.
            } else {
                0.
            })
        .max(1.);
        let core_gap = (available / core_count * 0.14).min(4.);
        let core_width = (available / core_count - core_gap).clamp(0.1, 25.);
        let mut panel = div()
            .flex()
            .flex_col()
            .w_full()
            .min_h_full()
            .px(px(15.))
            .py(px(13.))
            .gap(px(15.))
            .rounded(px(chrome::RADIUS - 1.))
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
            .font_family(chrome::FONT)
            .text_size(px(12.));
        let header = div()
            .id("metrics-header")
            .relative()
            .h(px(28.))
            .flex()
            .items_center()
            .justify_between()
            .child(
                self.figure(
                    "▦",
                    if self.data.cores.is_empty() {
                        "— %".into()
                    } else {
                        format!("{:.0} %", self.data.cpu)
                    },
                    accent,
                )
                .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
                .text_size(px(if size.width < px(340.) { 14. } else { 17. }))
                .font_weight(FontWeight::SEMIBOLD),
            )
            .when(
                self.settings.cores && !compact && self.data.gpu_usage.is_some(),
                |header| {
                    header.child(
                        div()
                            .absolute()
                            .left(px((core_count * (core_width + core_gap) + 10.)
                                .min(f32::from(size.width) - 174.)
                                .max(66.)))
                            .top(px(0.))
                            .h(px(28.))
                            .flex()
                            .items_center()
                            .child(
                                self.figure(
                                    "▧",
                                    self.data
                                        .gpu_usage
                                        .map(|v| format!("{v:.0} %"))
                                        .unwrap_or_else(|| "—".into()),
                                    self.gpu_color(),
                                )
                                .text_size(px(if size.width < px(340.) { 14. } else { 17. }))
                                .font_weight(FontWeight::SEMIBOLD),
                            ),
                    )
                },
            )
            .child(self.window_controls(cx, false));
        let mut readings = div().flex().flex_col().gap(px(10.)).flex_1().min_w(px(0.));
        if self.settings.cores && !compact {
            readings = readings.child(
                div()
                    .flex()
                    .items_end()
                    .gap(px(core_gap))
                    .children(self.data.cores.iter().enumerate().map(|(i, c)| {
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(3.))
                            .child(self.bar(
                                self.displayed_usage.get(i).copied().unwrap_or(c.usage),
                                (c.mhz, c.max_mhz),
                                core_width,
                                core_height,
                                self.core_color(c.group),
                            ))
                            .when(core_width >= 17., |el| {
                                el.child(
                                    div()
                                        .text_size(px(10.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(format!("{:.0}", c.usage)),
                                )
                                .child(
                                    div().text_size(px(8.5)).text_color(rgb(muted)).child(
                                        if c.mhz == 0 {
                                            "—".into()
                                        } else {
                                            format!("{:.1}", c.mhz as f32 / 1000.)
                                        },
                                    ),
                                )
                            })
                            .id(("core", i))
                            .tooltip({
                                let detail = self.t(&format!(
                                    "Core {} · {:.1}% · reported {} MHz",
                                    c.id + 1,
                                    c.usage,
                                    c.mhz
                                ));
                                move |_, cx| chrome::tooltip(detail.clone(), cx)
                            })
                    }))
                    .when(self.data.gpu_usage.is_some(), |row| {
                        row.child(self.gpu_column(gpu_width, core_height, core_width >= 17.))
                    }),
            );
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
                        memory_color,
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
                                .bg(rgb(memory_color)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .text_color(rgb(muted))
                        .text_size(px(10.5))
                        .child(
                            self.data
                                .compressed
                                .map(|bytes| {
                                    format!(
                                        "{} {} GB",
                                        self.data
                                            .compressed_label
                                            .as_deref()
                                            .unwrap_or("Compressed"),
                                        gb(bytes)
                                    )
                                })
                                .map(|text| self.t(&text))
                                .unwrap_or_else(|| self.t("Compressed memory unavailable")),
                        )
                        .child(format!(
                            "{} {} / {} GB",
                            if self.data.swap_label.is_empty() {
                                self.t("Swap")
                            } else {
                                self.t(&self.data.swap_label)
                            },
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
                .map(|text| self.t(&text))
                .unwrap_or_else(|| self.t("Temperature unavailable"));
            let mut gpu = self
                .data
                .gpu_usage
                .map(|usage| format!("{usage:.0} %"))
                .unwrap_or_else(|| "不可用".into());
            if let Some(bytes) = self.data.gpu_memory {
                gpu.push_str(&format!(" · {} GB", gb(bytes)));
            }
            readings = readings
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .text_color(rgb(muted))
                        .child(self.figure("▧", gpu, self.gpu_color()))
                        .child(
                            self.figure(
                                "ϟ",
                                self.data
                                    .gpu_power
                                    .map(|watts| format!("{watts:.1} W"))
                                    .unwrap_or_else(|| "不可用".into()),
                                self.gpu_color(),
                            ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .text_color(rgb(muted))
                        .child(
                            self.figure(
                                "⌂",
                                self.data
                                    .power
                                    .map(|watts| format!("{watts:.1} W"))
                                    .unwrap_or_else(|| "不可用".into()),
                                muted,
                            ),
                        )
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
                                            .unwrap_or_else(|| "不可用".into()),
                                        muted,
                                    ),
                                )
                                .tooltip(move |_, cx| chrome::tooltip(sensor.clone(), cx)),
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
                    .child(self.t("Reading system metrics…")),
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
                            "{}   {:.1}% CPU   {} GB   {} {}",
                            p.name,
                            p.cpu,
                            gb(p.memory),
                            p.pids.len(),
                            self.t("processes")
                        )),
                );
            }
        }
        if let Some(error) = &self.data.error {
            panel = panel.child(self.t(error));
        }
        if let Some(error) = &self.error {
            panel = panel.child(format!("{} {error}", self.t("Could not save settings:")));
        }
        let content = div()
            .id("monitor")
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(chrome::RADIUS - 1.))
            .bg(if self.settings.light {
                rgba(0xf1f4f7ed)
            } else {
                rgba(0x181923ed)
            })
            .font_family(chrome::FONT)
            .text_size(px(12.))
            .text_color(rgb(if self.settings.light {
                0x182130
            } else {
                0xe4e5ec
            }))
            .on_key_down(cx.listener(Self::key_down))
            .child(
                div()
                    .flex_none()
                    .px(px(15.))
                    .pt(px(13.))
                    .pb(px(2.))
                    .child(header),
            )
            .child(
                div()
                    .id("panel-scroll")
                    .flex_1()
                    .min_h(px(0.))
                    .w_full()
                    .overflow_y_scroll()
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, _, _, cx| {
                            this.controls = true;
                            this.install_guide |= this.updates.version.is_some();
                            this.updates.dismiss();
                            cx.notify();
                        }),
                    )
                    .child(panel),
            );
        chrome::frame(content, window)
    }
}
fn main() {
    if std::env::args().any(|a| a == "--version") {
        println!("ftop {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if std::env::args().any(|a| a == "--diagnostics") {
        let mut collector = metrics::Collector::new();
        let _ = collector.sample();
        std::thread::sleep(Duration::from_secs(1));
        let s = collector.sample();
        println!(
            "{}",
            serde_json::json!({"gpu_usage":s.gpu_usage,"gpu_memory":s.gpu_memory,"gpu_frequency":s.gpu_frequency,"gpu_power":s.gpu_power,"gpu_temperature":s.gpu_temperature,"gpu_info":s.gpu_info,"power":s.power,"power_label":s.power_label,"temperature":s.temperature,"compressed":s.compressed,"compressed_label":s.compressed_label,"memory_pressure":s.memory_pressure_info,"capabilities":s.capabilities})
        );
        return;
    }
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            cx.set_app_identity("ftop", "ftop");
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
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
                    window.set_app_id("ftop");
                    window.set_window_title("ftop");
                    cx.new(|cx| Monitor::new(window, cx))
                },
            )
            .expect("Unable to open ftop window");
            cx.activate(true);
        });
}
