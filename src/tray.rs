use crate::icon;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub enum TrayAction {
    ToggleVisible,
    ToggleLock,
    Quit,
}

pub struct Tray {
    icon: TrayIcon,
    id_visible: MenuId,
    id_lock: MenuId,
    id_quit: MenuId,
    shown: Option<bool>,
    pub last_error: Option<String>,
}

fn make(locked: bool) -> Option<Icon> {
    let b = icon::badge(locked, 32);
    Icon::from_rgba(b.px, b.w, b.h).ok()
}

impl Tray {
    pub fn new() -> Result<Self, String> {
        let menu = Menu::new();
        let visible = MenuItem::new("표시 / 숨김\tCtrl+Shift+X", true, None);
        let lock = MenuItem::new("고정 / 해제\tCtrl+Shift+Q", true, None);
        let quit = MenuItem::new("종료", true, None);

        menu.append_items(&[&visible, &lock, &PredefinedMenuItem::separator(), &quit])
            .map_err(|e| e.to_string())?;

        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip("keenpin — LOCKED")
            .with_icon(make(true).ok_or("아이콘 생성 실패")?)
            .build()
            .map_err(|e| e.to_string())?;

        Ok(Self {
            icon,
            id_visible: visible.id().clone(),
            id_lock: lock.id().clone(),
            id_quit: quit.id().clone(),
            shown: Some(true),
            last_error: None,
        })
    }

    /// 아이콘 모양·색으로 고정 상태를 표시한다. 값이 바뀔 때만 갱신.
    pub fn reflect(&mut self, locked: bool) {
        if self.shown == Some(locked) {
            return;
        }
        self.shown = Some(locked);

        match make(locked) {
            Some(i) => {
                if let Err(e) = self.icon.set_icon(Some(i)) {
                    self.last_error = Some(e.to_string());
                }
            }
            None => self.last_error = Some("아이콘 생성 실패".into()),
        }

        let _ = self.icon.set_tooltip(Some(if locked {
            "keenpin — LOCKED"
        } else {
            "keenpin — RELEASED"
        }));
    }

    pub fn drain(&self) -> Vec<TrayAction> {
        let mut out = Vec::new();

        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = ev
            {
                out.push(TrayAction::ToggleVisible);
            }
        }

        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if ev.id == self.id_visible {
                out.push(TrayAction::ToggleVisible);
            } else if ev.id == self.id_lock {
                out.push(TrayAction::ToggleLock);
            } else if ev.id == self.id_quit {
                out.push(TrayAction::Quit);
            }
        }

        out
    }
}
