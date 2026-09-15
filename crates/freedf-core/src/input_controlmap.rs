//! 사용자 매핑 테이블 (계약 객체 ③ — ideation `idea4/control-map.js`의 이식).
//!
//! 버튼의 "의미"는 하드웨어가 아니라 이 **데이터**가 결정한다. 직렬화 가능한
//! 순수 데이터 — 설정 파일에 그대로 저장되는 형태다. 매핑 없는 컨트롤은
//! None — 호출자는 조용히 무시한다 (미지원 하드웨어 안전).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::input_events::{
    ActionMode, ActionSource, ControlEvent, ControlKind, ControlPhase, InputEvent,
};

/// 바인딩 모드 — 탭(즉시 실행) 또는 홀드(누르는 동안 툴 교체).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BindingMode {
    Tap,
    Hold,
}

/// 컨트롤 1개의 바인딩 — (action 키, 모드).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlBinding {
    pub action: String,
    pub mode: BindingMode,
}

/// 컨트롤의 안정 식별자 — `(종류, 번호)` 쌍을 문자열로 ("stylus-button:1").
/// 버튼 "개수"와 무관하게 들어온 (종류, 번호)만 조회한다.
pub fn control_key(control: ControlKind, index: u8) -> String {
    let kind = match control {
        ControlKind::StylusButton => "stylus-button",
        ControlKind::ExpressKey => "express-key",
    };
    format!("{kind}:{index}")
}

/// 사용자 매핑 테이블.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ControlMap {
    bindings: HashMap<String, ControlBinding>,
}

impl ControlMap {
    /// 기본 매핑 — 현재 앱 배선과 동일: 버튼 1 탭 = 원형 색상 팔레트 토글.
    /// (버튼 2는 예약 — 미바인딩)
    pub fn with_defaults() -> Self {
        let mut map = Self::default();
        map.set(
            ControlKind::StylusButton,
            1,
            Some(ControlBinding {
                action: "color-wheel".into(),
                mode: BindingMode::Tap,
            }),
        );
        map
    }

    /// 매핑 조회 (설정 UI용).
    pub fn binding(&self, control: ControlKind, index: u8) -> Option<&ControlBinding> {
        self.bindings.get(&control_key(control, index))
    }

    /// 매핑 설정/해제 (설정 UI용).
    pub fn set(&mut self, control: ControlKind, index: u8, binding: Option<ControlBinding>) {
        let key = control_key(control, index);
        match binding {
            Some(b) => {
                self.bindings.insert(key, b);
            }
            None => {
                self.bindings.remove(&key);
            }
        }
    }

    /// 원시 컨트롤 → action 번역.
    ///
    /// 탭은 Down에서만, 홀드는 Down=HoldOn/Up=HoldOff로 발화한다.
    pub fn translate(&self, ev: &ControlEvent) -> Option<InputEvent> {
        let b = self.binding(ev.control, ev.index)?;
        let source = ActionSource::Control(ev.control);
        match (b.mode, ev.phase) {
            (BindingMode::Tap, ControlPhase::Down) => Some(InputEvent::action(
                source,
                b.action.clone(),
                Some(ActionMode::Tap),
            )),
            (BindingMode::Hold, ControlPhase::Down) => Some(InputEvent::action(
                source,
                b.action.clone(),
                Some(ActionMode::HoldOn),
            )),
            (BindingMode::Hold, ControlPhase::Up) => Some(InputEvent::action(
                source,
                b.action.clone(),
                Some(ActionMode::HoldOff),
            )),
            (BindingMode::Tap, ControlPhase::Up) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input_events::ControlEvent;

    fn ev(control: ControlKind, index: u8, phase: ControlPhase) -> ControlEvent {
        ControlEvent {
            control,
            index,
            phase,
        }
    }

    #[test]
    fn default_button1_is_tap_color_wheel() {
        let map = ControlMap::with_defaults();
        let a = map.translate(&ev(ControlKind::StylusButton, 1, ControlPhase::Down));
        match a {
            Some(InputEvent::Action(act)) => {
                assert_eq!(act.key, "color-wheel");
                assert_eq!(act.mode, Some(ActionMode::Tap));
            }
            other => panic!("action이 나와야 한다: {other:?}"),
        }
        // 탭은 Up에서 발화하지 않는다.
        assert!(map
            .translate(&ev(ControlKind::StylusButton, 1, ControlPhase::Up))
            .is_none());
        // 미바인딩은 조용히 무시.
        assert!(map
            .translate(&ev(ControlKind::StylusButton, 2, ControlPhase::Down))
            .is_none());
    }

    #[test]
    fn hold_binding_speaks_both_edges() {
        let mut map = ControlMap::default();
        map.set(
            ControlKind::StylusButton,
            2,
            Some(ControlBinding {
                action: "tool:eraser".into(),
                mode: BindingMode::Hold,
            }),
        );
        let on = map.translate(&ev(ControlKind::StylusButton, 2, ControlPhase::Down));
        let off = map.translate(&ev(ControlKind::StylusButton, 2, ControlPhase::Up));
        assert!(matches!(
            on,
            Some(InputEvent::Action(ref a)) if a.mode == Some(ActionMode::HoldOn)
        ));
        assert!(matches!(
            off,
            Some(InputEvent::Action(ref a)) if a.mode == Some(ActionMode::HoldOff)
        ));
    }

    #[test]
    fn control_key_is_stable_per_identity_pair() {
        assert_eq!(control_key(ControlKind::StylusButton, 1), "stylus-button:1");
        assert_eq!(control_key(ControlKind::ExpressKey, 12), "express-key:12");
    }
}
