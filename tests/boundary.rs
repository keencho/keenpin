//! SPEC.md §1 안전 경계를 빌드 타임에 강제한다.
//! 이 테스트가 깨지면 keenpin 은 더 이상 "자기 창만 만지는 프로그램"이 아니다.

use std::fs;
use std::path::Path;

const FORBIDDEN: &[(&str, &str)] = &[
    ("SendInput", "입력 합성"),
    ("keybd_event", "입력 합성"),
    ("mouse_event", "입력 합성"),
    ("SendMessage", "타 프로세스 메시지 큐 주입"),
    ("PostMessage", "타 프로세스 메시지 큐 주입"),
    ("PostThreadMessage", "타 스레드 큐 주입"),
    ("OpenProcess", "프로세스 핸들 획득"),
    ("ReadProcessMemory", "메모리 접근"),
    ("WriteProcessMemory", "메모리 접근"),
    ("VirtualAllocEx", "원격 메모리 할당"),
    ("CreateRemoteThread", "원격 스레드"),
    ("AttachThreadInput", "입력 큐 결합"),
    ("SetWindowsHookEx", "전역 후킹"),
    ("RegisterRawInputDevices", "원시 입력 수집"),
    ("GetAsyncKeyState", "전역 키 상태 폴링"),
    ("GetKeyboardState", "전역 키 상태 폴링"),
];

fn rust_sources(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            rust_sources(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn no_foreign_process_access() {
    let mut files = Vec::new();
    rust_sources(Path::new("src"), &mut files);
    assert!(!files.is_empty(), "src/ 에 소스가 없다");

    let mut violations = Vec::new();

    for path in &files {
        let src = fs::read_to_string(path).unwrap();
        for (line_no, line) in src.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            for (sym, why) in FORBIDDEN {
                if code.contains(sym) {
                    violations.push(format!(
                        "  {}:{} — {} ({})",
                        path.display(),
                        line_no + 1,
                        sym,
                        why
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "\n\n안전 경계 위반 (SPEC.md §1 Tier C):\n{}\n\n\
         keenpin 은 자기 창 외의 대상에 작용하지 않는다.\n\
         이 기능이 정말 필요하다면 그것은 keenpin 이 아니라 다른 프로그램이다.\n",
        violations.join("\n")
    );
}

/// unsafe 는 win/ 안에만 존재해야 한다.
#[test]
fn unsafe_is_contained() {
    let mut files = Vec::new();
    rust_sources(Path::new("src"), &mut files);

    let leaked: Vec<_> = files
        .iter()
        .filter(|p| !p.components().any(|c| c.as_os_str() == "win"))
        .filter(|p| {
            fs::read_to_string(p)
                .unwrap()
                .lines()
                .any(|l| l.split("//").next().unwrap_or("").contains("unsafe"))
        })
        .map(|p| p.display().to_string())
        .collect();

    assert!(
        leaked.is_empty(),
        "\nunsafe 는 src/win/ 안에만 허용된다. 위반:\n  {}\n",
        leaked.join("\n  ")
    );
}

/// 남의 창에 쓰기를 하는 코드는 win/passive.rs 안에만 있어야 한다.
/// 그 파일이 유일한 감사 지점이 되도록 강제한다.
#[test]
fn foreign_window_writes_are_contained() {
    const MARKERS: &[&str] = &["EnumWindows", "GetWindowThreadProcessId"];

    let mut files = Vec::new();
    rust_sources(Path::new("src"), &mut files);

    let leaked: Vec<_> = files
        .iter()
        .filter(|p| p.file_name().is_none_or(|f| f != "passive.rs"))
        .filter(|p| {
            let src = fs::read_to_string(p).unwrap();
            src.lines()
                .map(|l| l.split("//").next().unwrap_or(""))
                .any(|code| MARKERS.iter().any(|m| code.contains(m)))
        })
        .map(|p| p.display().to_string())
        .collect();

    assert!(
        leaked.is_empty(),
        "\n타 프로세스 창 열거/조작은 src/win/passive.rs 안에만 허용된다. 위반:\n  {}\n",
        leaked.join("\n  ")
    );
}

/// 포커스 되찾기는 win/reclaim.rs 안에만 있어야 한다.
/// 이 파일 하나만 감사하면 "포커스를 강제로 옮기는 코드"를 전부 볼 수 있다.
#[test]
fn foreground_control_is_contained() {
    const MARKERS: &[&str] = &[
        "SetForegroundWindow",
        "BringWindowToTop",
        "SystemParametersInfoW",
        "FOREGROUNDLOCKTIMEOUT",
    ];

    let mut files = Vec::new();
    rust_sources(Path::new("src"), &mut files);

    let leaked: Vec<_> = files
        .iter()
        .filter(|p| p.file_name().is_none_or(|f| f != "reclaim.rs"))
        .filter(|p| {
            fs::read_to_string(p)
                .unwrap()
                .lines()
                .map(|l| l.split("//").next().unwrap_or(""))
                .any(|code| MARKERS.iter().any(|m| code.contains(m)))
        })
        .map(|p| p.display().to_string())
        .collect();

    assert!(
        leaked.is_empty(),
        "\n포커스 강제 이동은 src/win/reclaim.rs 안에만 허용된다. 위반:\n  {}\n",
        leaked.join("\n  ")
    );
}
