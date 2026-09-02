-- FreeDF v2 — 0002: 확장성 (미디어 / 영속 히스토리 / 전문 검색)
-- JSON 파일 저장소 시대에는 불가능했던 미래 대비 계층입니다.
-- 기존 앱 데이터에 영향을 주지 않고 추가만 합니다.

-- ── 미디어/첨부 파일 (이미지·오디오·임의 파일) ──────────────────────────────
-- 아직 앱 UI에는 노출되지 않지만, 클라우드/미디어 기능의 저장 대상으로
-- 스키마가 먼저 준비됩니다. 바이너리는 BYTEA(단일 진실 공급원) + 메타데이터.
CREATE TABLE media (
    id          BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    doc_id      BIGINT REFERENCES documents(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL,        -- 'image' | 'audio' | 'video' | 'file'
    name        TEXT NOT NULL,
    mime        TEXT,
    size_bytes  BIGINT NOT NULL DEFAULT 0,
    data        BYTEA NOT NULL,
    created_at  BIGINT NOT NULL,
    updated_at  BIGINT NOT NULL
);
CREATE INDEX media_doc_idx ON media (doc_id);
CREATE INDEX media_kind_idx ON media (kind);

-- ── 영속 편집 저널 (undo/redo 히스토리의 DB화) ───────────────────────────────
-- 그리기/지우기/전체 지우기 한 번 = 행 하나(Edit JSONB). 재시작 후 이 저널을
-- 재생해 undo 스택을 복원합니다. 문서당 최근 500건 유지(앱이 트리밍).
CREATE TABLE doc_edits (
    id          BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    doc_id      BIGINT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    edit        JSONB NOT NULL,
    created_at  BIGINT NOT NULL
);
CREATE INDEX doc_edits_doc_idx ON doc_edits (doc_id, id);

-- ── 라이브러리 전문 검색 (제목) ──────────────────────────────────────────────
-- 표현식 GIN 인덱스 — 컬럼/트리거 없이 기존 쿼리는 그대로 두고
-- 나중에 `WHERE to_tsvector('simple', title) @@ websearch_to_tsquery('simple', ?)`로 검색.
CREATE INDEX documents_title_fts_idx
    ON documents USING gin (to_tsvector('simple', title));
