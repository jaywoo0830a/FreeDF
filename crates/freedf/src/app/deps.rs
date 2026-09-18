//! AppDeps — 앱의 **서비스 컴포지션 루**.
//!
//! - 펜 프로파일은 per-session 데이터라 `Materials` 값으로 명시 주입 (DI 컨테이너 대상 아님).
//! - 여기는 **교차 횡단(cross-cutting) 서비스**인 `Clock`/`Logger`를 한 번 묶어
//!   컴포지션 루(=`main`)에서 조립하고 `FreeDfApp::new`에 주입합니다.
//! - 테스트는 [`AppDeps::for_tests`]로 `FakeClock` + 무동작 로거를 갈아 끼웁니다.
//!
//! bake 서비스(`BakeService`)는 이미 **비제네릭(인터페이스 기반)** 이지만, 세션 설정(메셔)에
//! 묶여 있어 여기 담지 않습니다 — `FreeDfApp::new`가 메셔를 만든 뒤 조립합니다.

use freedf_canvas::clock::Clock;
use freedf_core::logging::Logger;

/// 조립된 서비스 묶음 — `main`(루트)에서 `compose`로 만들고 앱에 주입.
pub struct AppDeps {
    /// 단조 증가 시각 공급자 (테스트는 `FakeClock`을 주입).
    pub clock: Box<dyn Clock + Send + Sync>,
    /// 구조적 이벤트 로거 (연결 전이면 no-op 싱크).
    pub logger: Logger,
}

impl AppDeps {
    /// 컴포지션 루 — 실제 벽시계(`SystemClock`) + 운영 로거로 조립.
    pub fn compose(clock: Box<dyn Clock + Send + Sync>, logger: Logger) -> Self {
        Self { clock, logger }
    }
}
