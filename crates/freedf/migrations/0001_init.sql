-- FreeDF v2 — PostgreSQL 스키마 (JSON 파일 저장소의 완전 대체)
-- 모든 데이터(노트/PDF 본문/주석/용지/세션/최근/로그)가 이 스키마에 저장됩니다.

-- ── 문서 (노트 + 외부 PDF 공통) ────────────────────────────────────────────
CREATE TABLE documents (
    id          BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    kind        TEXT NOT NULL CHECK (kind IN ('note', 'pdf')),
    title       TEXT NOT NULL,
    origin_path TEXT,                 -- 외부 PDF의 원래 경로 (노트는 NULL)
    pdf         BYTEA,                -- PDF 본문 바이트 (단일 진실 공급원)
    page_count  INT NOT NULL DEFAULT 0,
    created_at  BIGINT NOT NULL,      -- epoch ms
    updated_at  BIGINT NOT NULL
);

-- ── 페이지별 용지 + 북마크 ──────────────────────────────────────────────────
CREATE TABLE pages (
    doc_id      BIGINT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    page_index  INT NOT NULL,
    style       TEXT NOT NULL DEFAULT 'Blank',
    color       INT[] NOT NULL,       -- [r, g, b, a]
    spacing     REAL NOT NULL DEFAULT 24,
    line_color  INT[] NOT NULL,       -- [r, g, b, a]
    line_width  REAL NOT NULL DEFAULT 1.2,
    bookmarked  BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (doc_id, page_index)
);

-- ── 스트로크 (전역 시퀀스로 id 부여 → 스토어/히스토리 id와 일치) ──────────
CREATE SEQUENCE stroke_id_seq;

CREATE TABLE strokes (
    id          BIGINT PRIMARY KEY DEFAULT nextval('stroke_id_seq'),
    doc_id      BIGINT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    page_index  INT NOT NULL,
    tool        TEXT NOT NULL,
    color       INT[] NOT NULL,       -- [r, g, b, a]
    width       REAL NOT NULL,
    points      JSONB NOT NULL,       -- [{x, y, pressure}, ...]
    created_at  BIGINT NOT NULL
);
CREATE INDEX strokes_doc_page_idx ON strokes (doc_id, page_index);

-- ── 문서별 GUI 세션 ─────────────────────────────────────────────────────────
CREATE TABLE sessions (
    doc_id      BIGINT PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
    state       JSONB NOT NULL,
    updated_at  BIGINT NOT NULL
);

-- ── 전역 앱 상태 (기본 세션 등) ────────────────────────────────────────────
CREATE TABLE app_state (
    key         TEXT PRIMARY KEY,
    value       JSONB NOT NULL
);

-- ── 최근 항목 ───────────────────────────────────────────────────────────────
CREATE TABLE recents (
    kind        TEXT NOT NULL CHECK (kind IN ('note', 'pdf')),
    doc_id      BIGINT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    title       TEXT NOT NULL,
    opened_at   BIGINT NOT NULL,
    PRIMARY KEY (kind, doc_id)
);
CREATE INDEX recents_opened_idx ON recents (opened_at DESC);

-- ── 분석 이벤트 로그 ────────────────────────────────────────────────────────
CREATE TABLE event_log (
    seq         BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    epoch_ms    BIGINT NOT NULL,
    event       JSONB NOT NULL
);
