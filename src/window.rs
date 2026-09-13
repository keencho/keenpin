//! 창 위치·크기·펼침 상태를 다음 실행까지 기억한다.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
pub struct Saved {
    pub x: Option<i32>,
    pub y: Option<i32>,
    /// 펼쳤을 때 높이. 물리 픽셀.
    pub expanded_h: Option<i32>,
    pub expanded: bool,
}

impl Default for Saved {
    fn default() -> Self {
        Self {
            x: None,
            y: None,
            expanded_h: None,
            expanded: true,
        }
    }
}

fn path() -> PathBuf {
    std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("keenpin")
        .join("window.json")
}

pub fn load() -> Saved {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(s: &Saved) {
    let p = path();
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string(s) {
        let _ = std::fs::write(p, text);
    }
}
