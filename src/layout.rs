//! UI 토큰. 크기·색을 여기서만 정한다.

use egui::Color32;

pub const W: f32 = 264.0;
pub const H_COLLAPSED: f32 = 78.0;
pub const H_EXPANDED: f32 = 470.0;
pub const H_MIN: f32 = 300.0;
pub const H_MAX: f32 = 900.0;

/// 글자 크기는 세 단계만 쓴다.
pub const FT_HEAD: f32 = 13.0;
pub const FT_BODY: f32 = 11.0;
pub const FT_SUB: f32 = 9.5;

pub const BG: Color32 = Color32::from_rgb(20, 20, 26);
pub const PANEL: Color32 = Color32::from_rgb(28, 28, 36);
pub const OK: Color32 = Color32::from_rgb(78, 222, 128);
pub const WARN: Color32 = Color32::from_rgb(242, 98, 98);
pub const DIM: Color32 = Color32::from_gray(140);
pub const TEXT: Color32 = Color32::from_gray(215);

pub fn head(text: impl Into<String>, color: Color32) -> egui::RichText {
    egui::RichText::new(text)
        .size(FT_HEAD)
        .color(color)
        .strong()
}

pub fn body(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).size(FT_BODY).color(TEXT)
}

pub fn sub(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).size(FT_SUB).color(DIM)
}
