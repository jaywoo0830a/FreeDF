//! Font setup — freedf의 임베드 폰트(Asta Sans + Inter + NanumGothic)를
//! 공유한다 (한글/CJK 글리프). Phosphor 아이콘은 freedf-gui가 아직 쓰지 않는다.

use eframe::egui;
use std::sync::Arc;

/// Asta Sans Regular — freedf 크레이트의 에셋을 상대 경로로 공유.
const ASTA_SANS_REGULAR: &[u8] =
    include_bytes!("../../freedf/assets/fonts/AstaSans-Regular.ttf");
const INTER_REGULAR: &[u8] = include_bytes!("../../freedf/assets/fonts/Inter-Regular.ttf");
const NANUM_GOTHIC_REGULAR: &[u8] =
    include_bytes!("../../freedf/assets/fonts/NanumGothic-Regular.ttf");

/// freedf `fonts.rs::install_inter`와 동일 구성 (아이콘 폰트 제외).
pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "asta_sans".to_owned(),
        Arc::new(egui::FontData::from_static(ASTA_SANS_REGULAR)),
    );
    fonts.font_data.insert(
        "inter".to_owned(),
        Arc::new(egui::FontData::from_static(INTER_REGULAR)),
    );
    fonts.font_data.insert(
        "nanum_gothic".to_owned(),
        Arc::new(egui::FontData::from_static(NANUM_GOTHIC_REGULAR)),
    );

    let mut prop = vec![
        "asta_sans".to_owned(),
        "inter".to_owned(),
        "nanum_gothic".to_owned(),
    ];
    if let Some(existing) = fonts.families.get(&egui::FontFamily::Proportional) {
        for name in existing {
            if !prop.contains(name) {
                prop.push(name.clone());
            }
        }
    }
    fonts.families.insert(egui::FontFamily::Proportional, prop);

    let mut mono = vec![
        "inter".to_owned(),
        "asta_sans".to_owned(),
        "nanum_gothic".to_owned(),
    ];
    if let Some(existing) = fonts.families.get(&egui::FontFamily::Monospace) {
        for name in existing {
            if !mono.contains(name) {
                mono.push(name.clone());
            }
        }
    }
    fonts.families.insert(egui::FontFamily::Monospace, mono);

    ctx.set_fonts(fonts);
}
