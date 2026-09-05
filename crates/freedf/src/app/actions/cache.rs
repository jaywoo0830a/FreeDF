//! cache — 정리 가능한 앱 캐시의 공통 인터페이스 [`Cacheable`].
//!
//! 새 캐시를 추가하려면 [`Cacheable`]을 구현하고 [`all_caches`]에 등록만 하면
//! 툴바 메뉴에 자동으로 나타납니다 (UI 코드 수정 불필요).

use super::*;

/// 정리 가능한 캐시 — 다운로드(디스크)/캔버스(메모리) 등 앱의 모든 캐시가
/// 따르는 공통 인터페이스. (`Sync` — 레지스트리가 정적 트레이트 객체를
/// 보관하므로 구현체도 자동으로 Sync여야 합니다.)
pub(crate) trait Cacheable: Sync {
    /// 캐시 이름 — 툴바 메뉴의 섹션 제목.
    fn label(&self) -> &'static str;

    /// 사용자에게 보여줄 짧은 설명.
    fn description(&self) -> &'static str;

    /// 캐시 정리 실행. 앱 상태(상태줄/백그라운드 채널/텍스처)에 접근할 수
    /// 있어야 하므로 `FreeDfApp`을 받습니다. 완료 메시지는 `app.status`에
    /// 직접 남깁니다.
    fn clear(&self, app: &mut FreeDfApp);
}

/// 등록된 캐시 목록 — 새 캐시는 여기에만 추가하면 UI가 자동 반영됩니다.
pub(crate) fn all_caches() -> &'static [&'static dyn Cacheable] {
    ALL_CACHES
}

static DOWNLOAD_CACHE: &dyn Cacheable = &DownloadCache;
static CANVAS_CACHE: &dyn Cacheable = &CanvasCache;
static ALL_CACHES: &[&dyn Cacheable] = &[DOWNLOAD_CACHE, CANVAS_CACHE];

/// 다운로드 캐시 (디스크) — 서버 PDF 미러 + 임시 미디어 파일.
pub(crate) struct DownloadCache;

impl Cacheable for DownloadCache {
    fn label(&self) -> &'static str {
        "Download cache"
    }

    fn description(&self) -> &'static str {
        "Server PDFs mirrored on disk and temporary media files."
    }

    fn clear(&self, app: &mut FreeDfApp) {
        if app.cache_rx.is_some() {
            app.status = Some("Cache clearing is already running.".into());
            return;
        }
        // 녹음 중이면 임시 WAV를 건드리지 않도록 거부.
        if app.recording.is_some() {
            app.status = Some("Stop the recording before clearing the cache.".into());
            return;
        }
        // 디스크 IO는 백그라운드 스레드 — UI를 막지 않습니다.
        let (tx, rx) = std::sync::mpsc::channel();
        app.cache_rx = Some(rx);
        app.begin_loading("Clearing download cache…");
        std::thread::spawn(move || {
            let dirs = [
                // 서버 PDF 미러 (<app_data>/v3_cache/pdfs — 문서 원본은 서버에 있음).
                crate::storage::app_data_dir().join("v3_cache").join("pdfs"),
                // 인앱 스트리밍 재생 임시 파일.
                std::env::temp_dir().join("freedf-stream"),
                // "기본 앱으로 열기" 임시 파일.
                std::env::temp_dir().join("freedf-open"),
                // 녹음 임시 WAV.
                std::env::temp_dir().join("freedf-recordings"),
            ];
            let mut freed: u64 = 0;
            for d in &dirs {
                freed += dir_size(d);
            }
            for d in &dirs {
                let _ = std::fs::remove_dir_all(d);
            }
            let msg = if freed > 0 {
                format!("Download cache cleared — freed {}.", fmt_bytes(freed))
            } else {
                "Download cache cleared — nothing to remove.".into()
            };
            let _ = tx.send(Ok(msg));
        });
    }
}

/// 캔버스 캐시 (메모리) — 페이지 텍스처/프리페치/잉크 메시.
pub(crate) struct CanvasCache;

impl Cacheable for CanvasCache {
    fn label(&self) -> &'static str {
        "Canvas cache"
    }

    fn description(&self) -> &'static str {
        "In-memory page textures and ink meshes — re-rendered from source on the next frame."
    }

    fn clear(&self, app: &mut FreeDfApp) {
        // 동기 — 메모리 해제만 하므로 즉시 완료됩니다. 다음 프레임에
        // 원본(벡터 획 + PDF)에서 다시 렌더링합니다.
        app.texture = None;
        app.prev_texture = None;
        app.prefetch = None;
        app.prefetch_pending = true;
        app.ink_mesh = None;
        app.ink_egui_mesh = None;
        app.ink_egui_key = None;
        app.active_mesh = None;
        app.ink_next_settle_ms = u64::MAX;
        app.render_dirty = true;
        app.status = Some("Canvas cache cleared — pages will re-render.".into());
    }
}

/// 디렉터리 총 크기 (바이트, 재귀 — 없는 경로는 0).
fn dir_size(path: &std::path::Path) -> u64 {
    fn walk(p: &std::path::Path, acc: &mut u64) {
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                let path = e.path();
                if path.is_dir() {
                    walk(&path, acc);
                } else if let Ok(md) = e.metadata() {
                    *acc += md.len();
                }
            }
        }
    }
    let mut n = 0;
    walk(path, &mut n);
    n
}

/// 사람이 읽기 쉬운 바이트 표기.
fn fmt_bytes(n: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    if n >= MB as u64 {
        format!("{:.1} MB", n as f64 / MB)
    } else if n >= KB as u64 {
        format!("{:.1} KB", n as f64 / KB)
    } else {
        format!("{n} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 레지스트리에 두 갈래 캐시가 모두 등록되어 있어야 합니다
    /// (새 캐시를 추가할 때 이 목록에 넣는 계약).
    #[test]
    fn all_caches_registers_both_kinds() {
        let caches = all_caches();
        assert_eq!(caches.len(), 2);
        let labels: Vec<_> = caches.iter().map(|c| c.label()).collect();
        assert_eq!(labels, vec!["Download cache", "Canvas cache"]);
        assert!(caches.iter().all(|c| !c.description().is_empty()));
    }
}
