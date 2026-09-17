//! elm-magic PoC — fallback_dialog의 본문을 [`elm_magic::view!`] 컴포넌트로 그린다.
//!
//! elm-magic("상태는 변수, 이벤트는 대입, 화면은 함수 본문")을 FreeDF의 기존
//! egui 프레임 안에서 쓰는 **파일럿**입니다. 윈도우 타이틀/폭/여백 리듬은
//! 기존 [`crate::ui::dialog`]를 그대로 재사용하고, **내용물만** elm-magic
//! 어댑터(`elm_magic_egui::render`)가 egui 위젯으로 렌더링합니다.
//!
//! 동작 방식:
//! - 모달이 열려 있는 동안 [`elm_magic::Ctx`]를 유지합니다. 컴포넌트 매개변수는
//!   아레나 슬롯으로 승격되고(선언 순서 = 슬롯 인덱스), 타이핑/클릭 같은 이벤트는
//!   슬롯 대입(`on_click={ok = true}`)으로 기록됩니다.
//! - 매 프레임 [`elm_magic::frame`]으로 `Element` 트리를 만들고 어댑터가 그립니다.
//! - 결과(text/ok/cancel)는 슬롯을 읽어 [`ElmModalOut`]으로 돌려줍니다.
//! - kind가 바뀌면 슬롯 레이아웃도 바뀌므로 Ctx를 폐기하고 새로 만듭니다.
//!
//! PoC 한계 (라이브러리 미구현 어휘 때문):
//! - 액션 행이 우측 정렬이 아니라 좌측부터 흐릅니다 (어댑터 `Row`가
//!   `horizontal` 한정 — 우측 정렬 레이아웃 어휘 없음).
//! - NewNote의 페이지 수 ComboBox는 elm-magic에 어휘가 없어 호출부(app/mod.rs)가
//!   egui로 직접 그립니다.
//! - `ui::form::text`의 자체 스타일/포커스 링이 아니라 어댑터의 순수
//!   `egui::TextEdit`로 그려집니다.

use eframe::egui;

elm_magic::view! {
    fn AskTextBody(hint = String::new(), text = String::new(), ok = false, cancel = false) {
        <Col>
            "{hint}"
            <Input value={text} on_change={text = _} on_enter={ok = true}></Input>
            <Row>
                <Button on_click={cancel = true}>"Cancel"</Button>
                <Button on_click={ok = true}>"OK"</Button>
            </Row>
        </Col>
    }
}

elm_magic::view! {
    fn ConfirmBody(message = String::new(), ok = false, cancel = false) {
        <Col>
            "{message}"
            <Row>
                <Button on_click={cancel = true}>"Cancel"</Button>
                <Button on_click={ok = true}>"Delete"</Button>
            </Row>
        </Col>
    }
}

elm_magic::view! {
    fn AlertBody(message = String::new(), ok = false) {
        <Col>
            "{message}"
            <Row>
                <Button on_click={ok = true}>"OK"</Button>
            </Row>
        </Col>
    }
}

/// 한 프레임의 렌더 결과 — 호출부(fallback_dialog)가 소비합니다.
pub(crate) struct ElmModalOut {
    /// 입력 슬롯의 현재 값 (AskText 전용 — Confirm/Alert은 빈 문자열).
    pub text: String,
    pub ok: bool,
    pub cancel: bool,
}

/// 모달 세션 슬롯(`App.modal_elm`)에서 이 모달 kind의 Ctx를 꺼낸다.
///
/// kind 키가 다르면(= 다른 모달) 상태를 폐기하고 새 Ctx를 만든다. 모달이
/// 닫힐 때 호출부가 슬롯을 비운다.
fn session_ctx<'a>(
    slot: &'a mut Option<(String, elm_magic::Ctx)>,
    kind_key: &str,
) -> &'a mut elm_magic::Ctx {
    if !matches!(slot, Some((k, _)) if k == kind_key) {
        *slot = Some((kind_key.to_string(), elm_magic::Ctx::default()));
    }
    &mut slot.as_mut().unwrap().1
}

/// AskText 모달 본문 (hint + 입력 필드 + Cancel/OK 행).
///
/// 슬롯 배치: `hint=0, text=1, ok=2, cancel=3` (`AskTextBody` 매개변수 순서).
pub(crate) fn render_ask_text(
    ui: &mut egui::Ui,
    slot: &mut Option<(String, elm_magic::Ctx)>,
    kind_key: &str,
    hint: &str,
    initial_text: &str,
) -> ElmModalOut {
    let ctx = session_ctx(slot, kind_key);
    let props = AskTextBodyProps {
        hint: Some(hint.to_string()),
        text: Some(initial_text.to_string()),
        ..Default::default()
    };
    let tree = elm_magic::frame::<AskTextBody>(ctx, &props);
    let _pass = elm_magic_egui::render(ui, &tree, &mut ctx.arena);
    ElmModalOut {
        text: ctx.arena.get::<String>(1).clone(),
        ok: *ctx.arena.get::<bool>(2),
        cancel: *ctx.arena.get::<bool>(3),
    }
}

/// Confirm 모달 본문 (message + Cancel/Delete 행).
///
/// 슬롯 배치: `message=0, ok=1, cancel=2`.
pub(crate) fn render_confirm(
    ui: &mut egui::Ui,
    slot: &mut Option<(String, elm_magic::Ctx)>,
    kind_key: &str,
    message: &str,
) -> ElmModalOut {
    let ctx = session_ctx(slot, kind_key);
    let props = ConfirmBodyProps {
        message: Some(message.to_string()),
        ..Default::default()
    };
    let tree = elm_magic::frame::<ConfirmBody>(ctx, &props);
    let _pass = elm_magic_egui::render(ui, &tree, &mut ctx.arena);
    ElmModalOut {
        text: String::new(),
        ok: *ctx.arena.get::<bool>(1),
        cancel: *ctx.arena.get::<bool>(2),
    }
}

/// Alert 모달 본문 (message + OK 행).
///
/// 슬롯 배치: `message=0, ok=1`.
pub(crate) fn render_alert(
    ui: &mut egui::Ui,
    slot: &mut Option<(String, elm_magic::Ctx)>,
    kind_key: &str,
    message: &str,
) -> ElmModalOut {
    let ctx = session_ctx(slot, kind_key);
    let props = AlertBodyProps {
        message: Some(message.to_string()),
        ..Default::default()
    };
    let tree = elm_magic::frame::<AlertBody>(ctx, &props);
    let _pass = elm_magic_egui::render(ui, &tree, &mut ctx.arena);
    ElmModalOut {
        text: String::new(),
        ok: *ctx.arena.get::<bool>(1),
        cancel: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 헤드리스 마운트로 슬롯 인덱스 계약을 검증한다 — 이 계약이 어긋나면
    /// app/mod.rs의 슬롯 읽기가 전부 틀어진다.
    #[test]
    fn confirm_click_sets_ok_slot() {
        let mut app = elm_magic::mount!(ConfirmBody);
        app.click("Delete");
        assert!(*app.ctx.arena.get::<bool>(1), "Delete → ok 슬롯");
        assert!(!*app.ctx.arena.get::<bool>(2));
    }

    #[test]
    fn confirm_cancel_sets_cancel_slot() {
        let mut app = elm_magic::mount!(ConfirmBody);
        app.click("Cancel");
        assert!(*app.ctx.arena.get::<bool>(2), "Cancel → cancel 슬롯");
        assert!(!*app.ctx.arena.get::<bool>(1));
    }

    #[test]
    fn alert_click_sets_ok_slot() {
        let mut app = elm_magic::mount!(AlertBody);
        app.click("OK");
        assert!(*app.ctx.arena.get::<bool>(1));
    }

    #[test]
    fn ask_text_ok_and_cancel_slots() {
        let mut app = elm_magic::mount!(AskTextBody);
        app.click("OK");
        assert!(*app.ctx.arena.get::<bool>(2));
        let mut app = elm_magic::mount!(AskTextBody);
        app.click("Cancel");
        assert!(*app.ctx.arena.get::<bool>(3));
        assert!(!*app.ctx.arena.get::<bool>(2));
    }
}
