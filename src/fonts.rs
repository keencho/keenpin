use std::sync::Arc;

const CANDIDATES: &[&str] = &[
    r"C:\Windows\Fonts\malgun.ttf",
    r"C:\Windows\Fonts\NanumGothic.ttf",
    r"C:\Windows\Fonts\NanumBarunGothic.ttf",
];

/// egui 기본 폰트에는 한글 글리프가 없다. 시스템 폰트를 폴백으로 붙인다.
pub fn install(ctx: &egui::Context) {
    let Some(bytes) = CANDIDATES.iter().find_map(|p| std::fs::read(p).ok()) else {
        return;
    };

    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("kr".to_owned(), Arc::new(egui::FontData::from_owned(bytes)));

    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("kr".to_owned());
    }

    ctx.set_fonts(fonts);
}
