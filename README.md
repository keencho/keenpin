# keenpin

클릭해도 활성 창을 뺏지 않는 Windows 오버레이 위젯.

## 동작

창에 `WS_EX_NOACTIVATE` 를 걸면 클릭해도 OS가 활성화 단계를 건너뛴다.
고정을 켜면 그 시점의 활성 창을 유지 대상으로 잡고, 체크한 앱의 창에 같은 플래그를 건다.
변경 전 확장 스타일은 창별로 기록했다가 해제할 때 되돌린다. 비정상 종료해도 다음 실행에서 복구한다.

일부 앱은 스스로 `SetForegroundWindow` 를 호출한다 — Chromium 계열이 페이지를 옮길 때 그렇게 한다.
이때는 유지 대상으로 포커스를 되돌린다. 체크한 앱이 가져갔을 때만 동작하므로 UAC나 시스템 대화상자는
건드리지 않고, 분당 30회를 넘으면 포커스 싸움으로 보고 스스로 멈춘다.

## 하지 않는 것

- 입력 생성 — `SendInput`, `keybd_event`, `PostMessage(WM_KEY*)`
- 프로세스 핸들 획득, 메모리 읽기/쓰기
- 전역 후킹, `AttachThreadInput`, DLL 인젝션

`tests/boundary.rs` 가 소스를 검사해 강제한다. 위반하면 빌드가 깨진다.
타 프로세스 창에 쓰는 코드는 `win/passive.rs`, 포커스를 옮기는 코드는 `win/reclaim.rs` 안에만 있다.

## 단축키

| | |
|---|---|
| `Ctrl+Shift+X` | 표시 / 숨김 |
| `Ctrl+Shift+Q` | 고정 / 해제 |

종료는 트레이 우클릭.

## 빌드

```
cargo build --release
```

`target/release/keenpin.exe` 하나로 돌아간다. Windows 10 1809 이상.

설정은 `%LOCALAPPDATA%\keenpin\` 에 저장된다.
