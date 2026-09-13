//! 내 창의 크기·위치. 대상은 항상 keenpin 자신의 창이다 (Tier A).
//! 가상 화면 범위 조회는 읽기 전용 (Tier B).

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, GetWindowRect, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOZORDER, SetWindowPos,
};

/// (x, y, w, h) 물리 픽셀.
pub fn rect(hwnd: HWND) -> (i32, i32, i32, i32) {
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(hwnd, &mut r);
    }
    (r.left, r.top, r.right - r.left, r.bottom - r.top)
}

pub fn resize(hwnd: HWND, w: i32, h: i32) {
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            w.max(1),
            h.max(1),
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_NOACTIVATE,
        );
    }
}

/// 모든 모니터를 합친 범위. 모니터 구성이 바뀌어 창이 화면 밖에 저장돼 있을 때 쓴다.
fn virtual_screen() -> (i32, i32, i32, i32) {
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}

/// 창이 완전히 화면 밖이면 안쪽으로 끌어온다. 최소 한 귀퉁이는 보이게.
pub fn clamp_visible(x: i32, y: i32, w: i32, _h: i32) -> (i32, i32) {
    let (vx, vy, vw, vh) = virtual_screen();
    if vw <= 0 || vh <= 0 {
        return (x, y);
    }
    const EDGE: i32 = 48;
    let nx = x.clamp(vx - w + EDGE, vx + vw - EDGE);
    let ny = y.clamp(vy, vy + vh - EDGE);
    (nx, ny)
}
