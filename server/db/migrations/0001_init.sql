-- FreeDF — 0001: 핵심 스키마 (PostgreSQL 18.6, SSD 튜닝 반영)
-- 모든 데이터(노트/PDF 본문/주석/용지/세션/최근/로그)가 여기 저장됩니다.
-- 캐시성/재생성 가능한 테이블(recents, event_log)은 UNLOGGED —
-- WAL 쓰기를 건너뛰어 SSD 쓰기 증폭을 줄입니다 (크래시 시 해당 테이블만 비워짐).

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

-- 노트 목록은 updated_at DESC 정렬이 메인 경로.
CREATE INDEX documents_kind_updated_idx ON documents (kind, updated_at DESC);

-- ── 페이지별 용지 + 북마크 ──────────────────────────────────────────────────
CREATE TABLE pages (
    doc_id      BIGINT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    page_index  INT NOT NULL,
    style       TEXT NOT NULL DEFAULT 'Blank',
    color       INT[] NOT NULL,       -- [r, g, b, a]
    bookmarked  BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (doc_id, page_index)
);

-- ── 스트로크 (전역 시퀀스로 id 부여 → 스토어/히스토리 id와 일치) ──────────
-- CACHE 100: 여러 클라이언트가 nextval을 자주 호출해도 락 경합이 적음.
CREATE SEQUENCE stroke_id_seq CACHE 100;

CREATE TABLE strokes (
    id          BIGINT PRIMARY KEY DEFAULT nextval('stroke_id_seq'),
    doc_id      BIGINT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    page_index  INT NOT NULL,
    tool        TEXT NOT NULL,
    color       INT[] NOT NULL,       -- [r, g, b, a]
    width       REAL NOT NULL,
    points      JSONB NOT NULL,       -- [{x, y, pressure, ...}, ...]
    created_at  BIGINT NOT NULL
);
-- load_store: WHERE doc_id ORDER BY id → 이 인덱스로 그룹 스캔.
CREATE INDEX strokes_doc_id_idx ON strokes (doc_id, id);
-- 페이지 단위 조회/카운트.
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

-- ── 최근 항목 (UNLOGGED — documents에서 재구성 가능한 캐시) ───────────────
CREATE UNLOGGED TABLE recents (
    kind        TEXT NOT NULL CHECK (kind IN ('note', 'pdf')),
    doc_id      BIGINT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    title       TEXT NOT NULL,
    opened_at   BIGINT NOT NULL,
    PRIMARY KEY (kind, doc_id)
);
CREATE INDEX recents_opened_idx ON recents (opened_at DESC);

-- ── 분석 이벤트 로그 (UNLOGGED + BRIN) ──────────────────────────────────────
-- append-only라 BRIN이 B-tree보다 훨씬 작고 빠릅니다.
CREATE UNLOGGED TABLE event_log (
    seq         BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    epoch_ms    BIGINT NOT NULL,
    event       JSONB NOT NULL
);
CREATE INDEX event_log_epoch_brin ON event_log USING brin (epoch_ms);
