use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey, Modifiers},
};

pub const VISIBLE_LABEL: &str = "Ctrl+Shift+X";
pub const TOGGLE_LABEL: &str = "Ctrl+Shift+Q";

pub enum Action {
    ToggleVisible,
    ToggleLock,
}

pub struct Hotkeys {
    _manager: GlobalHotKeyManager,
    visible: u32,
    toggle: u32,
}

impl Hotkeys {
    pub fn new() -> Result<Self, global_hotkey::Error> {
        let manager = GlobalHotKeyManager::new()?;
        let m = Modifiers::CONTROL | Modifiers::SHIFT;

        let visible = HotKey::new(Some(m), Code::KeyX);
        let toggle = HotKey::new(Some(m), Code::KeyQ);

        manager.register_all(&[visible, toggle])?;

        Ok(Self {
            _manager: manager,
            visible: visible.id(),
            toggle: toggle.id(),
        })
    }

    pub fn drain(&self) -> Vec<Action> {
        let rx = GlobalHotKeyEvent::receiver();
        let mut out = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            if ev.state != HotKeyState::Pressed {
                continue;
            }
            if ev.id == self.visible {
                out.push(Action::ToggleVisible);
            } else if ev.id == self.toggle {
                out.push(Action::ToggleLock);
            }
        }
        out
    }
}
