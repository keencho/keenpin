use crate::hotkey::{Action, Hotkeys, TOGGLE_LABEL, VISIBLE_LABEL};
use crate::layout as L;
use crate::tray::{Tray, TrayAction};
use crate::win::overlay::Overlay;
use crate::win::passive::{AppGroup, Passive};
use crate::win::reclaim::{self, Reclaim, State};
use crate::win::{drag, focus, geom, noactivate};
use crate::{fonts, window};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;

const SWEEP_EVERY: Duration = Duration::from_millis(250);

enum Grab {
    Move { dx: i32, dy: i32 },
    Resize { cx: i32, cy: i32, w: i32, h: i32 },
}

pub struct KeenPin {
    hk: Hotkeys,
    tray: Result<Tray, String>,
    overlay: Overlay,
    passive: Passive,
    reclaim: Reclaim,
    hwnd: Option<HWND>,
    saved: window::Saved,
    want_locked: bool,
    actually_locked: bool,
    visible: bool,
    keep_title: String,
    last_external: Option<(isize, String)>,
    groups: Vec<AppGroup>,
    last_sweep: Instant,
    started: bool,
    grab: Option<Grab>,
    notice: Option<String>,
}

impl KeenPin {
    fn new(cc: &eframe::CreationContext<'_>, hk: Hotkeys) -> Self {
        fonts::install(&cc.egui_ctx);

        let mut passive = Passive::new();
        if let Some(ms) = passive.recover_from_crash() {
            reclaim::force_restore(ms);
        }

        Self {
            hk,
            tray: Tray::new(),
            overlay: Overlay::new(),
            passive,
            reclaim: Reclaim::new(),
            hwnd: None,
            saved: window::load(),
            want_locked: false,
            actually_locked: false,
            visible: true,
            keep_title: String::new(),
            last_external: None,
            groups: Vec::new(),
            last_sweep: Instant::now(),
            started: false,
            grab: None,
            notice: None,
        }
    }

    fn ppp(ctx: &egui::Context) -> f32 {
        ctx.pixels_per_point().max(0.5)
    }

    fn collapsed_px(ctx: &egui::Context) -> i32 {
        (L::H_COLLAPSED * Self::ppp(ctx)).round() as i32
    }

    fn expanded_px(&self, ctx: &egui::Context) -> i32 {
        self.saved
            .expanded_h
            .unwrap_or_else(|| (L::H_EXPANDED * Self::ppp(ctx)).round() as i32)
    }

    fn apply_height(&mut self, ctx: &egui::Context) {
        let Some(hwnd) = self.hwnd else { return };
        let (_, _, w, _) = geom::rect(hwnd);
        let h = if self.saved.expanded {
            self.expanded_px(ctx)
        } else {
            Self::collapsed_px(ctx)
        };
        geom::resize(hwnd, w, h);
    }

    fn set_expanded(&mut self, ctx: &egui::Context, on: bool) {
        if self.saved.expanded == on {
            return;
        }
        if !on && let Some(hwnd) = self.hwnd {
            let (_, _, _, h) = geom::rect(hwnd);
            self.saved.expanded_h = Some(h);
        }
        self.saved.expanded = on;
        self.apply_height(ctx);
        window::save(&self.saved);
    }

    /// 저장된 위치·크기를 물리 픽셀로 적용한다.
    /// ViewportBuilder::with_position 은 논리 좌표라 DPI 배율에서 어긋난다.
    fn restore_geometry(&mut self, ctx: &egui::Context) {
        let Some(hwnd) = self.hwnd else { return };
        let (_, _, w, _) = geom::rect(hwnd);
        let h = if self.saved.expanded {
            self.expanded_px(ctx)
        } else {
            Self::collapsed_px(ctx)
        };
        geom::resize(hwnd, w, h);

        if let (Some(x), Some(y)) = (self.saved.x, self.saved.y) {
            let (cx, cy) = geom::clamp_visible(x, y, w, h);
            drag::move_to(hwnd, cx, cy);
        }
    }

    /// 이동·리사이즈 경로가 여럿이라 주기적으로 확인해 저장한다.
    /// 값이 바뀔 때만 디스크에 쓴다.
    fn sync_geometry(&mut self) {
        let Some(hwnd) = self.hwnd else { return };
        let (x, y, _, h) = geom::rect(hwnd);
        let mut dirty = self.saved.x != Some(x) || self.saved.y != Some(y);

        self.saved.x = Some(x);
        self.saved.y = Some(y);
        if self.saved.expanded && self.saved.expanded_h != Some(h) {
            self.saved.expanded_h = Some(h);
            dirty = true;
        }
        if dirty {
            window::save(&self.saved);
        }
    }

    fn bind_hwnd(&mut self, frame: &eframe::Frame) {
        if self.hwnd.is_some() {
            return;
        }
        let Ok(handle) = frame.window_handle() else {
            return;
        };
        if let RawWindowHandle::Win32(h) = handle.as_raw() {
            self.hwnd = Some(HWND(h.hwnd.get() as *mut core::ffi::c_void));
        }
    }

    /// 고정: 지금 포커스를 쥔 창을 유지 대상으로 삼고, 체크된 앱만 손 안 들게 만든다.
    fn set_locked(&mut self, locked: bool) {
        if self.want_locked == locked {
            return;
        }
        self.want_locked = locked;
        let own = self.hwnd.unwrap_or_default();

        if locked {
            // 버튼을 누르면 keenpin 이 잠깐 포그라운드가 될 수 있으므로
            // 직전에 기억해 둔 "keenpin 이 아닌 마지막 포그라운드" 를 대상으로 삼는다.
            let fg = focus::foreground();
            let target = if !fg.0.is_null() && fg.0 != own.0 {
                Some(fg)
            } else {
                self.last_external
                    .as_ref()
                    .map(|(k, _)| HWND(*k as *mut core::ffi::c_void))
            };

            let Some(target) = target else {
                self.want_locked = false;
                self.notice = Some("유지할 창을 먼저 클릭하세요".into());
                return;
            };

            self.keep_title = if target.0 == fg.0 {
                focus::foreground_title()
            } else {
                self.last_external
                    .as_ref()
                    .map(|(_, t)| t.clone())
                    .unwrap_or_default()
            };
            self.reclaim.engage();
            self.passive
                .engage(target, own, self.reclaim.saved_timeout());
            self.last_sweep = Instant::now();
            self.notice = None;
        } else {
            self.passive.release();
            self.reclaim.release();
            self.keep_title.clear();
        }
    }

    fn set_visible(&mut self, ctx: &egui::Context, visible: bool) {
        self.visible = visible;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(visible));
    }

    fn pump_events(&mut self, ctx: &egui::Context) {
        for a in self.hk.drain() {
            match a {
                Action::ToggleVisible => {
                    let v = !self.visible;
                    self.set_visible(ctx, v);
                }
                Action::ToggleLock => {
                    let next = !self.want_locked;
                    self.set_locked(next);
                }
            }
        }

        let actions = self.tray.as_ref().map(|t| t.drain()).unwrap_or_default();
        for a in actions {
            match a {
                TrayAction::ToggleVisible => {
                    let v = !self.visible;
                    self.set_visible(ctx, v);
                }
                TrayAction::ToggleLock => {
                    let next = !self.want_locked;
                    self.set_locked(next);
                }
                TrayAction::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            }
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        let color = if self.actually_locked { L::OK } else { L::WARN };
        let mut expand = false;
        let mut hide = false;

        let row1 = ui.horizontal(|ui| {
            ui.add_space(1.0);
            dot(ui, color);
            ui.label(L::head(
                if self.actually_locked {
                    "LOCKED"
                } else {
                    "RELEASED"
                },
                color,
            ));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let h = icon_button(ui, Glyph::Minimize, "트레이로 숨김");
                if h.clicked() {
                    hide = true;
                }
                let (g, tip) = if self.saved.expanded {
                    (Glyph::ChevronUp, "설정 접기")
                } else {
                    (Glyph::ChevronDown, "설정 펼치기")
                };
                let e = icon_button(ui, g, tip);
                if e.clicked() {
                    expand = true;
                }
                h.rect.left().min(e.rect.left())
            })
            .inner
        });

        let buttons_left = row1.inner;

        let body = ui
            .vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                if self.keep_title.is_empty() {
                    let pending = self
                        .last_external
                        .as_ref()
                        .map(|(_, t)| trim(t, 24))
                        .unwrap_or_else(|| "없음".to_owned());
                    ui.label(L::sub(format!("고정 대상: {pending}")));
                } else {
                    ui.label(
                        egui::RichText::new(trim(&self.keep_title, 30))
                            .size(L::FT_BODY)
                            .color(L::OK),
                    );
                }
                ui.horizontal(|ui| {
                    ui.label(L::sub(self.passive_summary()));
                    if self.reclaim.hits > 0 {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(L::sub(format!("되찾기 {}", self.reclaim.hits)));
                        });
                    }
                });
            })
            .response;

        // 버튼 위는 드래그 영역에서 뺀다. 겹치면 interact 가 클릭을 가로챈다.
        let mut top = row1.response.rect;
        top.max.x = buttons_left - 6.0;
        self.drag_move(ui, top, "move_top");
        self.drag_move(ui, body.rect, "move_body");

        let ctx = ui.ctx().clone();
        if expand {
            let next = !self.saved.expanded;
            self.set_expanded(&ctx, next);
        }
        if hide {
            self.set_visible(&ctx, false);
        }
    }

    /// 접힌 상태에서도 무엇이 걸려 있는지 보이게 한다.
    fn passive_summary(&self) -> String {
        let names: Vec<&str> = self
            .groups
            .iter()
            .filter(|g| g.selected && !g.has_keep)
            .map(|g| g.name.as_str())
            .collect();

        match names.len() {
            0 => "선택된 앱 없음".to_owned(),
            1..=2 => names.join(" · "),
            n => format!("{} 외 {}개", names[0], n - 1),
        }
    }

    /// 커서 절대좌표 기준. 창 위치를 계산에 넣지 않으므로 되먹임 진동이 없다.
    fn drag_move(&mut self, ui: &mut egui::Ui, zone: egui::Rect, id: &str) {
        let Some(hwnd) = self.hwnd else { return };
        if !zone.is_positive() {
            return;
        }
        let r = ui.interact(zone, ui.id().with(id), egui::Sense::drag());

        if r.hovered() || r.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
        if r.drag_started() {
            let (cx, cy) = drag::cursor();
            let (wx, wy, _, _) = geom::rect(hwnd);
            self.grab = Some(Grab::Move {
                dx: cx - wx,
                dy: cy - wy,
            });
        }
        if r.dragged()
            && let Some(Grab::Move { dx, dy }) = self.grab
        {
            let (cx, cy) = drag::cursor();
            drag::move_to(hwnd, cx - dx, cy - dy);
            ui.ctx().request_repaint();
        }
        if r.drag_stopped() {
            self.grab = None;
            self.sync_geometry();
        }
    }

    fn resize_grip(&mut self, ui: &mut egui::Ui) {
        let Some(hwnd) = self.hwnd else { return };
        let full = ui.max_rect();
        let zone = egui::Rect::from_min_max(
            egui::pos2(full.right() - 16.0, full.bottom() - 16.0),
            full.max,
        );

        let p = ui.painter();
        for i in 0..3 {
            let o = 3.0 + i as f32 * 3.5;
            p.line_segment(
                [
                    egui::pos2(zone.right() - o, zone.bottom() - 2.0),
                    egui::pos2(zone.right() - 2.0, zone.bottom() - o),
                ],
                egui::Stroke::new(1.0, egui::Color32::from_gray(80)),
            );
        }

        let r = ui.interact(zone, ui.id().with("resize"), egui::Sense::drag());
        if r.hovered() || r.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
        }
        if r.drag_started() {
            let (cx, cy) = drag::cursor();
            let (_, _, w, h) = geom::rect(hwnd);
            self.grab = Some(Grab::Resize { cx, cy, w, h });
        }
        if r.dragged()
            && let Some(Grab::Resize { cx, cy, w, h }) = self.grab
        {
            let (nx, ny) = drag::cursor();
            let ppp = Self::ppp(ui.ctx());
            let min_h = (L::H_MIN * ppp) as i32;
            let max_h = (L::H_MAX * ppp) as i32;
            let nw = (w + (nx - cx)).max((L::W * ppp) as i32);
            let nh = (h + (ny - cy)).clamp(min_h, max_h);
            geom::resize(hwnd, nw, nh);
            ui.ctx().request_repaint();
        }
        if r.drag_stopped() {
            self.grab = None;
            self.sync_geometry();
        }
    }

    fn picker(&mut self, ui: &mut egui::Ui) {
        let picked = self.groups.iter().filter(|g| g.selected).count();

        ui.horizontal(|ui| {
            ui.label(L::body("클릭해도 포커스 안 넘길 앱"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(L::sub(picked.to_string()));
            });
        });

        let mut toggled: Option<usize> = None;

        egui::Frame::NONE
            .fill(L::PANEL)
            .inner_margin(egui::Margin::symmetric(8, 6))
            .corner_radius(6.0)
            .show(ui, |ui| {
                ui.style_mut().spacing.scroll = egui::style::ScrollStyle::thin();
                ui.style_mut().spacing.item_spacing.y = 5.0;

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.groups.is_empty() {
                            ui.label(L::sub("창 없음"));
                        }
                        for (i, g) in self.groups.iter().enumerate() {
                            if g.has_keep {
                                ui.horizontal(|ui| {
                                    dot(ui, L::OK);
                                    ui.label(L::head(&g.name, L::OK).size(L::FT_BODY));
                                    ui.label(L::sub("유지 대상"));
                                });
                                continue;
                            }
                            ui.horizontal(|ui| {
                                let mut on = g.selected;
                                if ui.checkbox(&mut on, "").changed() {
                                    toggled = Some(i);
                                }
                                ui.label(L::body(trim(&g.name, 20)));
                                if g.keys.len() > 1 {
                                    ui.label(L::sub(format!("{}창", g.keys.len())));
                                }
                                let state = if g.applied > 0 {
                                    Some(Ok(()))
                                } else if g.selected {
                                    Some(Err(g
                                        .reason
                                        .clone()
                                        .unwrap_or_else(|| "적용 대기 중".to_owned())))
                                } else {
                                    None
                                };
                                if let Some(st) = state {
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| status_mark(ui, st.as_ref().err().map(|s| s.as_str())),
                                    );
                                }
                            });
                        }
                    });
            });

        if let Some(i) = toggled {
            let g = self.groups.swap_remove(i);
            self.passive.toggle_group(&g, self.reclaim.saved_timeout());
            if let Some(own) = self.hwnd {
                self.passive.sweep(own, self.reclaim.saved_timeout());
                self.groups = self.passive.groups(own);
            }
        }
    }
}

fn trim(s: &str, n: usize) -> String {
    let t: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        format!("{t}\u{2026}")
    } else {
        t
    }
}

impl eframe::App for KeenPin {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        let [r, g, b, _] = L::BG.to_normalized_gamma_f32();
        [r, g, b, 1.0]
    }

    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.bind_hwnd(frame);
        self.pump_events(ctx);

        let Some(hwnd) = self.hwnd else { return };
        if !self.started {
            self.passive.preselect_groups(hwnd);
            self.groups = self.passive.groups(hwnd);
            // 고른 앱이 없을 때만 강제로 펼친다. 그 외에는 저장된 상태를 따른다.
            if !self.groups.iter().any(|g| g.selected) {
                self.saved.expanded = true;
            }
            self.restore_geometry(ctx);
            self.started = true;
        }

        // keenpin 은 항상 손을 들지 않는다. 텍스트 입력이 없으므로 활성화될 이유가 없고,
        // 버튼을 눌러도 대상 창이 포커스를 유지해야 한다.
        noactivate::enforce(hwnd, true);
        self.actually_locked = self.want_locked && self.passive.engaged();

        // 고정 대상 후보: keenpin 이 아닌 마지막 포그라운드 창.
        let fg = focus::foreground();
        if !fg.0.is_null() && fg.0 != hwnd.0 {
            let key = fg.0 as isize;
            if self.last_external.as_ref().map(|(k, _)| *k) != Some(key) {
                self.last_external = Some((key, focus::foreground_title()));
            }
        }

        self.reclaim.tick();

        if self.last_sweep.elapsed() >= SWEEP_EVERY {
            self.groups = self.passive.groups(hwnd);
            if self.passive.engaged() {
                self.passive.sweep(hwnd, self.reclaim.saved_timeout());
            }
            self.sync_geometry();
            self.last_sweep = Instant::now();
        }

        if self.want_locked && !self.passive.keep_alive() {
            self.set_locked(false);
            self.notice = Some("유지 대상 창이 닫혀서 해제했습니다".into());
        }
        if self.want_locked && self.reclaim.runaway() {
            self.set_locked(false);
            self.notice = Some("되찾기 분당 30회 초과 — 자동 해제".into());
        }

        if self.want_locked
            && let Some(keep) = self.passive.keep_hwnd()
        {
            let fg = focus::foreground();
            if fg.0 != keep.0 && self.passive.holds(fg) {
                self.reclaim.to(keep);
            }
        }

        self.overlay.reflect(hwnd, self.actually_locked);
        if let Ok(t) = self.tray.as_mut() {
            t.reflect(self.actually_locked);
        }

        ctx.request_repaint_after(Duration::from_millis(80));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let shell = egui::Frame::NONE
            .fill(L::BG)
            .inner_margin(egui::Margin::symmetric(10, 8));

        egui::CentralPanel::default().frame(shell).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;

            self.header(ui);

            if !self.saved.expanded {
                return;
            }

            ui.separator();

            let btn = if self.want_locked {
                "해제 (Release)"
            } else {
                "고정 (Lock)"
            };
            if ui
                .add_sized([ui.available_width(), 26.0], egui::Button::new(btn))
                .clicked()
            {
                let next = !self.want_locked;
                self.set_locked(next);
            }

            match (&self.notice, self.reclaim.state()) {
                (Some(n), _) => {
                    ui.colored_label(L::WARN, egui::RichText::new(n).size(L::FT_SUB));
                }
                (None, State::Off) => {
                    ui.label(L::sub("유지할 창을 클릭해 포커스를 준 뒤 고정하세요"));
                }
                (None, State::Armed) => {
                    ui.colored_label(
                        L::OK,
                        egui::RichText::new(format!("되찾기 ON · 패시브 {}창", self.passive.count()))
                            .size(L::FT_SUB),
                    );
                }
                (None, State::Blocked) => {
                    ui.colored_label(
                        L::WARN,
                        egui::RichText::new("되찾기 실패 — 해제 후 이 버튼으로 다시 고정")
                            .size(L::FT_SUB),
                    );
                }
            }

            ui.separator();

            let footer = 46.0;
            let avail = (ui.available_height() - footer).max(80.0);
            ui.allocate_ui(egui::vec2(ui.available_width(), avail), |ui| {
                self.picker(ui);
            });

            ui.separator();
            ui.label(L::sub(format!(
                "{VISIBLE_LABEL}  표시 / 숨김      {TOGGLE_LABEL}  고정 / 해제\n종료는 트레이 우클릭"
            )));

            if let Err(e) = &self.tray {
                ui.colored_label(L::WARN, egui::RichText::new(format!("트레이 실패: {e}")).size(L::FT_SUB));
            }
        });

        if self.saved.expanded {
            self.resize_grip(ui);
        }
    }
}

pub fn run() -> eframe::Result {
    let hk = Hotkeys::new().expect("global hotkey 등록 실패");

    let vp = egui::ViewportBuilder::default()
        .with_title("keenpin")
        .with_app_id("keenpin")
        .with_inner_size([L::W, L::H_EXPANDED])
        .with_min_inner_size([L::W, L::H_COLLAPSED])
        .with_decorations(false)
        .with_transparent(false)
        .with_resizable(true)
        .with_always_on_top()
        .with_taskbar(true)
        .with_active(false);

    eframe::run_native(
        "keenpin",
        eframe::NativeOptions {
            viewport: vp,
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(KeenPin::new(cc, hk)))),
    )
}

#[derive(Clone, Copy)]
enum Glyph {
    Minimize,
    ChevronDown,
    ChevronUp,
}

/// 폰트 글리프 대신 직접 그린다. 폰트에 따라 두부가 되거나 크기가 들쭉날쭉한 것을 피한다.
fn icon_button(ui: &mut egui::Ui, g: Glyph, tip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(22.0, 18.0), egui::Sense::click());
    let p = ui.painter();

    if resp.hovered() {
        p.rect_filled(rect, 4.0, egui::Color32::from_gray(52));
    }
    let color = if resp.hovered() { L::TEXT } else { L::DIM };
    let s = egui::Stroke::new(1.4, color);
    let c = rect.center();

    match g {
        Glyph::Minimize => {
            p.line_segment(
                [
                    egui::pos2(c.x - 4.5, c.y + 1.0),
                    egui::pos2(c.x + 4.5, c.y + 1.0),
                ],
                s,
            );
        }
        Glyph::ChevronDown => {
            p.line_segment(
                [egui::pos2(c.x - 4.0, c.y - 2.0), egui::pos2(c.x, c.y + 2.0)],
                s,
            );
            p.line_segment(
                [egui::pos2(c.x, c.y + 2.0), egui::pos2(c.x + 4.0, c.y - 2.0)],
                s,
            );
        }
        Glyph::ChevronUp => {
            p.line_segment(
                [egui::pos2(c.x - 4.0, c.y + 2.0), egui::pos2(c.x, c.y - 2.0)],
                s,
            );
            p.line_segment(
                [egui::pos2(c.x, c.y - 2.0), egui::pos2(c.x + 4.0, c.y + 2.0)],
                s,
            );
        }
    }
    resp.on_hover_text(tip)
}

/// 상태 점. 폰트 글리프에 의존하지 않는다.
fn dot(ui: &mut egui::Ui, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.2, color);
}

/// 적용 여부 표시. 실패면 이유를 툴팁으로 보여준다.
fn status_mark(ui: &mut egui::Ui, fail: Option<&str>) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(15.0, 15.0), egui::Sense::hover());
    let p = ui.painter();
    let c = rect.center();

    if fail.is_none() {
        let s = egui::Stroke::new(1.7, L::OK);
        p.line_segment(
            [
                egui::pos2(c.x - 3.6, c.y + 0.2),
                egui::pos2(c.x - 1.1, c.y + 2.8),
            ],
            s,
        );
        p.line_segment(
            [
                egui::pos2(c.x - 1.1, c.y + 2.8),
                egui::pos2(c.x + 3.6, c.y - 2.8),
            ],
            s,
        );
        resp.on_hover_text("적용됨");
    } else {
        p.circle_filled(c, 5.0, L::WARN);
        p.line_segment(
            [egui::pos2(c.x, c.y - 2.4), egui::pos2(c.x, c.y + 0.6)],
            egui::Stroke::new(1.5, L::BG),
        );
        p.circle_filled(egui::pos2(c.x, c.y + 2.6), 0.9, L::BG);
        resp.on_hover_text(format!("적용 실패 — {}", fail.unwrap_or("원인 불명")));
    }
}
