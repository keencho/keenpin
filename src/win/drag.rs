//! 창 이동. 커서 절대좌표 기준으로 움직인다.
//!
//! egui 의 `drag_delta` 를 누적하는 방식은 진동한다:
//! 창을 옮기면 포인터의 창-로컬 좌표가 같이 변해서 다음 프레임 delta 에 되먹임된다.
//! 커서 스크린 좌표는 창 위치와 무관하므로 그 루프가 생기지 않는다.
//!
//! GetCursorPos / GetWindowRect 는 읽기 전용(Tier B), SetWindowPos 대상은 우리 창(Tier A).

use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetWindowRect, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER,
    SetWindowPos,
};

pub fn cursor() -> (i32, i32) {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    (p.x, p.y)
}

#[allow(dead_code)]
pub fn origin(hwnd: HWND) -> (i32, i32) {
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(hwnd, &mut r);
    }
    (r.left, r.top)
}

pub fn move_to(hwnd: HWND, x: i32, y: i32) {
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_NOACTIVATE,
        );
    }
}
