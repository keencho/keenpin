//! 유일한 unsafe 구역 중 하나. 여기서 다루는 HWND 는 항상 keenpin 자신의 창이다.
//! 외부 창 핸들을 이 모듈에 넘기지 말 것. SPEC.md §1 참조.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, SetWindowLongPtrW, SetWindowPos, WS_EX_NOACTIVATE,
};

fn ex_style(hwnd: HWND) -> u32 {
    unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 }
}

/// winit 이 창 생성 이후에도 확장 스타일을 덮어쓰므로 매 프레임 재확인한다.
/// 값이 이미 맞으면 아무것도 쓰지 않는다.
///
/// NOACTIVATE 외의 비트는 건드리지 않는다 — TOOLWINDOW 를 세우면
/// 작업표시줄에서 사라지는데, 그건 우리가 원하는 동작이 아니다.
pub fn enforce(hwnd: HWND, locked: bool) {
    let cur = ex_style(hwnd);
    let want = if locked {
        cur | WS_EX_NOACTIVATE.0
    } else {
        cur & !WS_EX_NOACTIVATE.0
    };

    if want == cur {
        return;
    }

    unsafe {
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, want as isize);
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE
                | SWP_NOSIZE
                | SWP_NOZORDER
                | SWP_NOOWNERZORDER
                | SWP_NOACTIVATE
                | SWP_FRAMECHANGED,
        );
    }
}

/// 의도가 아니라 실제 창 상태를 읽는다. UI 표시는 반드시 이 값을 쓸 것.
#[allow(dead_code)]
pub fn is_locked(hwnd: HWND) -> bool {
    ex_style(hwnd) & WS_EX_NOACTIVATE.0 != 0
}

#[allow(dead_code)]
pub fn describe(hwnd: HWND) -> String {
    format!("EXSTYLE 0x{:08X}", ex_style(hwnd))
}
