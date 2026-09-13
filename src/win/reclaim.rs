//! 뺏긴 포커스를 유지 대상으로 되돌린다.
//!
//! 배경: `WS_EX_NOACTIVATE` 는 클릭에 의한 활성화만 막는다. 앱이 스스로
//! `SetForegroundWindow` 를 부르는 것은 막지 못한다 (Chromium 이 페이지 이동 시 그렇게 한다).
//!
//! Windows 는 비포그라운드 프로세스의 `SetForegroundWindow` 를 거부한다.
//! 우회 경로가 셋인데 이 파일은 그 중 하나만 쓴다:
//!
//!   1. 가짜 ALT 입력 생성 — 가장 확실하지만 **입력 합성**이다. 쓰지 않는다.
//!   2. AttachThreadInput — 남의 입력 큐에 우리 큐를 결합. 쓰지 않는다.
//!   3. ForegroundLockTimeout = 0 — 문서화된 시스템 설정. **이것만 쓴다.**
//!
//! 3번은 GLFW 가 glfwInit 에서 하는 것과 같은 일이다. 원값을 저장했다가 되돌린다.
//! 단, 설정 변경 자체가 포그라운드 권한을 요구하므로 사용자가 고정을 누른
//! 그 순간(= keenpin 이 마지막 입력을 받은 프로세스인 순간)에 호출해야 한다.
//!
//! 되찾기 발동 조건은 "이미 패시브로 표시해 둔 창이 포커스를 가져갔을 때" 로 한정된다.
//! UAC·작업관리자·시스템 대화상자는 그 집합에 없으므로 건드리지 않는다. SPEC.md §10 참조.

use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    SPI_GETFOREGROUNDLOCKTIMEOUT, SPI_SETFOREGROUNDLOCKTIMEOUT, SPIF_SENDCHANGE,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SetForegroundWindow, SystemParametersInfoW,
};

const COOLDOWN: Duration = Duration::from_millis(120);
/// 분당 이 횟수를 넘으면 포커스 싸움으로 보고 스스로 멈춘다.
const RUNAWAY_PER_MIN: u32 = 30;

fn lock_timeout() -> Option<u32> {
    let mut v: u32 = 0;
    unsafe {
        SystemParametersInfoW(
            SPI_GETFOREGROUNDLOCKTIMEOUT,
            0,
            Some(&mut v as *mut u32 as *mut core::ffi::c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .ok()?;
    }
    Some(v)
}

fn set_lock_timeout(ms: u32) {
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_SETFOREGROUNDLOCKTIMEOUT,
            0,
            Some(ms as usize as *mut core::ffi::c_void),
            SPIF_SENDCHANGE,
        );
    }
}

#[derive(PartialEq, Clone, Copy)]
pub enum State {
    Off,
    /// 잠금시간 0 적용 확인됨. 되찾기가 실제로 동작한다.
    Armed,
    /// 설정 변경이 거부됨. 되찾기가 조용히 실패할 수 있다.
    Blocked,
}

pub struct Reclaim {
    saved: Option<u32>,
    state: State,
    last: Option<Instant>,
    window_started: Option<Instant>,
    pub hits: u32,
}

impl Reclaim {
    pub fn new() -> Self {
        Self {
            saved: None,
            state: State::Off,
            last: None,
            window_started: None,
            hits: 0,
        }
    }

    pub fn state(&self) -> State {
        self.state
    }

    /// 사용자가 고정을 누른 직후에만 호출할 것. 그때만 권한이 있다.
    pub fn engage(&mut self) {
        if self.saved.is_some() {
            return;
        }
        self.saved = lock_timeout();
        set_lock_timeout(0);

        // 조용히 실패하는 경우가 있으므로 반드시 읽어서 확인한다.
        self.state = match lock_timeout() {
            Some(0) => State::Armed,
            _ => State::Blocked,
        };
        self.hits = 0;
        self.window_started = Some(Instant::now());
    }

    pub fn release(&mut self) {
        if let Some(prev) = self.saved.take() {
            set_lock_timeout(prev);
        }
        self.state = State::Off;
        self.last = None;
        self.window_started = None;
    }

    /// 쿨다운을 둬서 포커스 싸움으로 화면이 떨리는 것을 막는다.
    pub fn to(&mut self, target: HWND) {
        if self.state == State::Off {
            return;
        }
        if self.last.is_some_and(|t| t.elapsed() < COOLDOWN) {
            return;
        }
        self.last = Some(Instant::now());
        self.hits += 1;
        unsafe {
            let _ = SetForegroundWindow(target);
        }
    }
}

impl Drop for Reclaim {
    fn drop(&mut self) {
        self.release();
    }
}

/// 크래시 복구용. 저장해 둔 원값을 되돌린다.
pub fn force_restore(ms: u32) {
    set_lock_timeout(ms);
}

impl Reclaim {
    /// 복구 파일에 남길 원값.
    pub fn saved_timeout(&self) -> Option<u32> {
        self.saved
    }

    /// 분당 임계치를 넘었는지. 넘으면 호출자가 고정을 해제한다.
    pub fn runaway(&self) -> bool {
        self.window_started
            .is_some_and(|t| t.elapsed() < Duration::from_secs(60) && self.hits > RUNAWAY_PER_MIN)
    }
}

impl Reclaim {
    /// 1분 창이 지나면 카운터를 리셋한다.
    pub fn tick(&mut self) {
        if self
            .window_started
            .is_some_and(|t| t.elapsed() >= Duration::from_secs(60))
        {
            self.window_started = Some(Instant::now());
            self.hits = 0;
        }
    }
}
