//! 사용자가 고른 창을 "손 안 드는 창"으로 만든다.
//!
//! 여기가 keenpin 에서 유일하게 남의 창에 쓰기를 하는 모듈이다.
//! 하는 일은 확장 스타일 비트를 세우는 것뿐이며, 원값을 기록해 두고 되돌린다.
//!
//! 대상은 **사용자가 체크한 창만** 이다. 자동 선택하지 않는다.
//! 체크하지 않은 창은 손대지 않으며, 그런 창이 포커스를 가져가면 되찾기도 하지 않는다.
//!
//! 하지 않는 것: 입력 생성, 메모리 접근, 프로세스 핸들 획득. SPEC.md §1, §9, §11 참조.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use windows::Win32::Foundation::{GetLastError, HWND, LPARAM, SetLastError, TRUE, WIN32_ERROR};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GWL_STYLE, GetClassNameW, GetWindow, GetWindowLongPtrW,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindow, IsWindowVisible,
    SetWindowLongPtrW, WS_CHILD, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};
use windows::core::BOOL;

/// 건드리면 데스크톱이 망가지는 셸 구성요소. 후보 목록에도 올리지 않는다.
const SKIP_CLASSES: &[&str] = &[
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "Progman",
    "WorkerW",
    "NotifyIconOverflowWindow",
    "Windows.UI.Core.CoreWindow",
    "MultitaskingViewFrame",
    "ForegroundStaging",
    "TaskListThumbnailWnd",
    "Xaml_WindowedPopupClass",
];

#[derive(Serialize, Deserialize, Default)]
pub struct Restore {
    /// 프로세스가 비정상 종료해도 되돌릴 수 있도록 디스크에 남긴다.
    pub fg_timeout: Option<u32>,
    pub windows: HashMap<String, u32>,
}

pub struct WindowInfo {
    pub key: isize,
    pub title: String,
    pub class: String,
    pub selected: bool,
    pub applied: bool,
    pub is_keep: bool,
}

struct Entry {
    /// 우리가 세운 것만 해제 시 되돌린다.
    ours: bool,
    original: u32,
    title: String,
}

fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn title_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 160];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn is_candidate(hwnd: HWND, own_pid: u32) -> bool {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return false;
        }
        if GetWindowTextLengthW(hwnd) == 0 {
            return false;
        }
        if GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.0.is_null()) {
            return false;
        }
        if GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 & WS_CHILD.0 != 0 {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == own_pid {
            return false;
        }
    }
    !SKIP_CLASSES.contains(&class_of(hwnd).as_str())
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let out = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
    out.push(hwnd);
    TRUE
}

fn top_level() -> Vec<HWND> {
    let mut out: Vec<HWND> = Vec::with_capacity(256);
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut out as *mut _ as isize));
    }
    out
}

fn ex_of(hwnd: HWND) -> u32 {
    unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 }
}

fn hwnd_of(key: isize) -> HWND {
    HWND(key as *mut core::ffi::c_void)
}

fn alive(hwnd: HWND) -> bool {
    unsafe { IsWindow(Some(hwnd)).as_bool() }
}

/// NOACTIVATE 는 작업표시줄 버튼을 없앤다(MSDN 명시).
/// APPWINDOW 를 같이 세워 버튼을 유지한다.
/// 원래 TOOLWINDOW 였던 창은 원래도 버튼이 없었으므로 그대로 둔다.
fn desired(cur: u32) -> u32 {
    let mut want = cur | WS_EX_NOACTIVATE.0;
    if cur & WS_EX_TOOLWINDOW.0 == 0 {
        want |= WS_EX_APPWINDOW.0;
    }
    want
}

pub struct Passive {
    applied: HashMap<isize, Entry>,
    failures: HashMap<isize, String>,
    /// 사용자가 체크한 창. 세션 한정.
    selected: HashSet<isize>,
    /// 다음 실행에서 같은 종류의 창을 자동 체크해 주기 위한 힌트.
    remembered: HashSet<String>,
    keep: Option<isize>,
    store: PathBuf,
    config: PathBuf,
}

impl Passive {
    pub fn new() -> Self {
        let dir = std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir())
            .join("keenpin");

        let mut p = Self {
            applied: HashMap::new(),
            failures: HashMap::new(),
            selected: HashSet::new(),
            remembered: HashSet::new(),
            keep: None,
            store: dir.join("restore.json"),
            config: dir.join("picks.json"),
        };
        p.load_picks();
        p
    }

    /// 비정상 종료로 남은 기록을 되돌린다. 반환값은 복구해야 할 포그라운드 잠금시간.
    pub fn recover_from_crash(&mut self) -> Option<u32> {
        let text = std::fs::read_to_string(&self.store).ok()?;
        let r: Restore = serde_json::from_str(&text).ok()?;
        for (k, v) in r.windows {
            if let Ok(h) = k.parse::<isize>() {
                restore_one(hwnd_of(h), v);
            }
        }
        let _ = std::fs::remove_file(&self.store);
        r.fg_timeout
    }

    fn load_picks(&mut self) {
        if let Ok(text) = std::fs::read_to_string(&self.config)
            && let Ok(set) = serde_json::from_str::<HashSet<String>>(&text)
        {
            self.remembered = set;
        }
    }

    fn save_picks(&self) {
        if let Some(dir) = self.config.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string(&self.remembered) {
            let _ = std::fs::write(&self.config, text);
        }
    }

    fn persist(&self, fg_timeout: Option<u32>) {
        if self.applied.is_empty() && fg_timeout.is_none() {
            let _ = std::fs::remove_file(&self.store);
            return;
        }
        if let Some(dir) = self.store.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let r = Restore {
            fg_timeout,
            windows: self
                .applied
                .iter()
                .filter(|(_, e)| e.ours)
                .map(|(k, e)| (k.to_string(), e.original))
                .collect(),
        };
        if let Ok(text) = serde_json::to_string(&r) {
            let _ = std::fs::write(&self.store, text);
        }
    }

    pub fn count(&self) -> usize {
        self.applied.len()
    }

    pub fn engaged(&self) -> bool {
        self.keep.is_some()
    }

    pub fn keep_hwnd(&self) -> Option<HWND> {
        self.keep.map(hwnd_of)
    }

    /// 되찾기 판단용. 우리가 실제로 손을 댄 창인가?
    pub fn holds(&self, hwnd: HWND) -> bool {
        self.applied.contains_key(&(hwnd.0 as isize))
    }

    /// 유지 대상 창이 살아 있는가.
    pub fn keep_alive(&self) -> bool {
        self.keep.is_none_or(|k| alive(hwnd_of(k)))
    }

    /// UI 용 후보 목록. 체크 상태와 실제 적용 여부를 함께 준다.
    pub fn scan(&self, own: HWND) -> Vec<WindowInfo> {
        let own_pid = unsafe { GetCurrentProcessId() };
        let mut v: Vec<_> = top_level()
            .into_iter()
            .filter(|h| h.0 != own.0 && is_candidate(*h, own_pid))
            .map(|h| {
                let key = h.0 as isize;
                WindowInfo {
                    key,
                    title: title_of(h),
                    class: class_of(h),
                    selected: self.selected.contains(&key),
                    applied: self.applied.contains_key(&key),
                    is_keep: self.keep == Some(key),
                }
            })
            .collect();
        v.sort_by_key(|w| w.title.to_lowercase());
        v
    }

    pub fn engage(&mut self, keep: HWND, own: HWND, fg_timeout: Option<u32>) {
        self.keep = Some(keep.0 as isize);
        self.selected.remove(&(keep.0 as isize));
        self.sweep(own, fg_timeout);
    }

    /// 체크된 창에만 적용하고, 스타일이 풀렸으면 다시 건다.
    /// Chromium / Electron 계열은 자체 이벤트에서 확장 스타일을 다시 쓰므로
    /// 한 번 걸어두는 것만으로는 유지되지 않는다.
    pub fn sweep(&mut self, own: HWND, fg_timeout: Option<u32>) {
        if self.keep.is_none() {
            return;
        }
        let keep = self.keep.unwrap();
        let mut changed = false;

        self.applied.retain(|k, e| {
            let h = hwnd_of(*k);
            if !alive(h) || !readable(h) {
                return false;
            }
            let cur = ex_of(h);
            if e.ours && cur & WS_EX_NOACTIVATE.0 == 0 {
                unsafe { SetWindowLongPtrW(h, GWL_EXSTYLE, desired(cur) as isize) };
            }
            if e.title.is_empty() {
                e.title = title_of(h);
            }
            true
        });

        self.failures.retain(|k, _| alive(hwnd_of(*k)));

        let wanted: Vec<isize> = self.selected.iter().copied().collect();
        for key in wanted {
            if key == keep || key == own.0 as isize || self.applied.contains_key(&key) {
                continue;
            }
            let h = hwnd_of(key);
            if !alive(h) {
                self.selected.remove(&key);
                continue;
            }
            match try_passive(h) {
                Ok(Applied::Ours(original)) => {
                    self.applied.insert(
                        key,
                        Entry {
                            original,
                            ours: true,
                            title: title_of(h),
                        },
                    );
                    self.failures.remove(&key);
                    changed = true;
                }
                Ok(Applied::Already) => {
                    self.applied.insert(
                        key,
                        Entry {
                            original: 0,
                            ours: false,
                            title: title_of(h),
                        },
                    );
                    self.failures.remove(&key);
                }
                Err(why) => {
                    self.failures.insert(key, why);
                }
            }
        }

        if changed {
            self.persist(fg_timeout);
        }
    }

    pub fn release(&mut self) {
        for (k, e) in self.applied.drain() {
            if e.ours {
                restore_one(hwnd_of(k), e.original);
            }
        }
        self.failures.clear();
        self.keep = None;
        self.persist(None);
    }
}

impl Drop for Passive {
    fn drop(&mut self) {
        self.release();
    }
}

/// 적용 결과. 실패 이유를 문자열로 남겨 UI 가 추측 없이 보여줄 수 있게 한다.
enum Applied {
    /// 우리가 세웠다. 해제 시 original 로 되돌린다.
    Ours(u32),
    /// 이미 켜져 있었다. 우리가 만든 상태가 아니므로 해제 시 건드리지 않는다.
    Already,
}

fn try_passive(hwnd: HWND) -> Result<Applied, String> {
    if !readable(hwnd) {
        let code = unsafe { GetLastError().0 };
        return Err(match code {
            5 => "창 접근 거부 (오류 5)".into(),
            0 => "창 핸들이 유효하지 않음".into(),
            n => format!("창 읽기 실패 (오류 {n})"),
        });
    }
    let cur = ex_of(hwnd);
    if cur & WS_EX_NOACTIVATE.0 != 0 {
        return Ok(Applied::Already);
    }

    unsafe {
        SetLastError(WIN32_ERROR(0));
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, desired(cur) as isize);
    }

    if ex_of(hwnd) & WS_EX_NOACTIVATE.0 != 0 {
        return Ok(Applied::Ours(cur));
    }

    let code = unsafe { GetLastError().0 };
    Err(match code {
        5 => "접근 거부 — 대상이 더 높은 권한으로 실행 중".into(),
        0 => "설정 직후 앱이 되돌림".into(),
        n => format!("SetWindowLongPtrW 실패 (오류 {n})"),
    })
}

fn restore_one(hwnd: HWND, original: u32) {
    if !alive(hwnd) || !readable(hwnd) {
        return;
    }
    unsafe { SetWindowLongPtrW(hwnd, GWL_EXSTYLE, original as isize) };
}

/// 창 제목에서 앱 이름을 뽑는다. Windows 관례상 "문서 - 앱이름" 형태가 많다.
/// 프로세스 핸들을 열면 실행 파일명을 정확히 알 수 있지만, 그건 게임 프로세스에
/// 핸들을 여는 것과 같은 동작이라 하지 않는다. SPEC.md §12 참조.
fn app_name(title: &str, class: &str) -> String {
    if let Some(i) = title.rfind(" - ") {
        let tail = title[i + 3..].trim();
        if !tail.is_empty() && tail.chars().count() <= 28 {
            return tail.to_owned();
        }
    }
    let t = title.trim();
    if t.is_empty() {
        class.to_owned()
    } else {
        t.chars().take(28).collect()
    }
}

pub struct AppGroup {
    pub key: String,
    pub name: String,
    pub keys: Vec<isize>,
    pub applied: usize,
    pub selected: bool,
    pub has_keep: bool,
    /// 적용에 실패한 경우의 이유. 추측하지 않고 GetLastError 기반으로 채운다.
    pub reason: Option<String>,
}

impl Passive {
    /// 창을 앱 단위로 묶는다. 크롬 탭이 바뀌어도 같은 그룹으로 남는다.
    pub fn groups(&self, own: HWND) -> Vec<AppGroup> {
        let mut by_key: HashMap<String, AppGroup> = HashMap::new();

        for w in self.scan(own) {
            let name = app_name(&w.title, &w.class);
            let key = format!("{}|{}", w.class, name);
            let g = by_key.entry(key.clone()).or_insert_with(|| AppGroup {
                key,
                name,
                keys: Vec::new(),
                applied: 0,
                selected: false,
                has_keep: false,
                reason: None,
            });
            g.keys.push(w.key);
            g.applied += usize::from(w.applied);
            g.selected |= w.selected;
            g.has_keep |= w.is_keep;
            if g.reason.is_none() {
                g.reason = self.failures.get(&w.key).cloned();
            }
        }

        let mut v: Vec<_> = by_key.into_values().collect();
        v.sort_by_key(|g| g.name.to_lowercase());
        v
    }

    pub fn toggle_group(&mut self, g: &AppGroup, fg_timeout: Option<u32>) {
        let turning_on = !g.selected;
        if turning_on {
            self.remembered.insert(g.key.clone());
            for k in &g.keys {
                self.selected.insert(*k);
            }
        } else {
            self.remembered.remove(&g.key);
            for k in &g.keys {
                self.selected.remove(k);
                self.failures.remove(k);
                if let Some(e) = self.applied.remove(k) {
                    restore_one(hwnd_of(*k), e.original);
                }
            }
        }
        self.save_picks();
        self.persist(fg_timeout);
    }

    /// 이전 실행에서 체크했던 앱을 자동으로 체크해 둔다. 적용은 하지 않는다.
    pub fn preselect_groups(&mut self, own: HWND) {
        let wanted: Vec<Vec<isize>> = self
            .groups(own)
            .into_iter()
            .filter(|g| self.remembered.contains(&g.key) && !g.has_keep)
            .map(|g| g.keys)
            .collect();
        for keys in wanted {
            for k in keys {
                self.selected.insert(k);
            }
        }
    }
}

/// 핸들 유효성 확인. 실제 창은 WS_VISIBLE 등이 있어 GWL_STYLE 이 0 일 수 없다.
/// 확장 스타일(GWL_EXSTYLE)은 0 이 정상값이므로 이 판정에 쓰면 안 된다.
fn readable(hwnd: HWND) -> bool {
    unsafe {
        SetLastError(WIN32_ERROR(0));
        GetWindowLongPtrW(hwnd, GWL_STYLE) != 0
    }
}
