//! build.rs — Windows 전용: 실행 파일(.exe) **자체의 아이콘**을 빌드 타임에
//! PE 리소스로 임베드합니다.
//!
//! 바탕화면·탐색기의 파일 아이콘과 작업 관리자 프로세스 아이콘은 모두 이
//! 실행 파일에 담긴 Group Icon 리소스에서 온 것입니다. (실행 중 창 아이콘은
//! 별개로 `src/icon.rs`의 `include_bytes!` PNG 를 eframe 창에 설정 — 런타임.)
//!
//! 임베드 방법: `embed-resource` 가 `win/app.rc` 를 windres / llvm-rc 로
//! 컴파일해 아이콘 리소스를 만들고, cargo 에게 링크 지시를 보냅니다.
//! `#[cfg(windows)]` 게이트 덕에 Windows 네이티브 빌드에서만 동작하고,
//! Linux 등 비 Windows 빌드는 no-op 이라 `embed-resource`(Windows 전용
//! build-dependency) 없이도 컴파일됩니다.

fn main() {
    // Windows 대상 네이티브 빌드에서만 리소스를 임베드합니다.
    #[cfg(windows)]
    {
        // 정적 .rc 를 컴파일 → .exe 에 app_icon.ico 를 아이콘 리소스로 링크.
        // 매니페스트는 아이콘처럼 "화장용(cosmetic)"이므로 manifest_optional().
        let _ = embed_resource::compile("win/app.rc", embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }
}