-- FreeDF — 0002: 영속 편집 저널(undo) + 제목 전문 검색
-- 미디어 바이너리는 BYTEA로 DB에 넣지 않고, 0004_media_objects +
-- nginx 정적 서빙(server/)으로 분리합니다.

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
