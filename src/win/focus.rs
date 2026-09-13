//! Tier B: 읽기 전용. 현재 포커스가 어디 있는지 표시하기 위한 용도.
//! 쓰기 API 없음. SPEC.md §1 참조.

use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};

pub fn foreground_title() -> String {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return "(없음)".to_owned();
        }
        let mut buf = [0u16; 256];
        let n = GetWindowTextW(hwnd, &mut buf);
        if n <= 0 {
            return "(제목 없음)".to_owned();
        }
        String::from_utf16_lossy(&buf[..n as usize])
    }
}

pub fn foreground() -> windows::Win32::Foundation::HWND {
    unsafe { GetForegroundWindow() }
}
