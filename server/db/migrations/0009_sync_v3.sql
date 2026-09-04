-- FreeDF — 0009: Sync v3 — 스냅샷 중심 동기화 (docs/sync-protocol-v3.md)
--
-- v3의 원칙: "클라이언트는 ZIP만, 나머지는 서버가".
--   doc_revisions : 문서별 단조 증가 revision (낙관적 동시성 기준)
--   doc_changelog : revision별 상태 diff (GET /changes?since_revision= 의 원천)
--   cas_objects   : 콘텐츠 주소 저장 — PDF/미디어 중복 전송·저장 제거
--   sync_uploads  : 비동기 업로드 작업 상태 (queued/processing/applied/conflict/failed)

-- 문서의 PDF 다이제스트 (CAS 참조).
ALTER TABLE documents ADD COLUMN IF NOT EXISTS pdf_digest TEXT;

-- ── 문서별 revision ────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS doc_revisions (
    doc_id      BIGINT PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
    revision    BIGINT NOT NULL DEFAULT 0,
    updated_at  BIGINT NOT NULL
);

-- ── 변경 로그 (revision별 diff) ─────────────────────────────────────────────
-- patch JSONB:
--   { from_revision, to_revision, strokes_added[], stroke_ids_removed[],
--     pages_changed, pages[], meta, pdf }
CREATE TABLE IF NOT EXISTS doc_changelog (
    id          BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    doc_id      BIGINT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    revision    BIGINT NOT NULL,
    patch       JSONB NOT NULL,
    created_at  BIGINT NOT NULL,
    UNIQUE (doc_id, revision)
);
CREATE INDEX IF NOT EXISTS doc_changelog_doc_rev_idx
    ON doc_changelog (doc_id, revision DESC);

-- ── 콘텐츠 주소 저장 (CAS) ──────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS cas_objects (
    digest      TEXT PRIMARY KEY,     -- "sha256:<hex64>"
    bytes       BYTEA NOT NULL,
    size        BIGINT NOT NULL,
    created_at  BIGINT NOT NULL
);

-- ── 비동기 업로드 작업 ──────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS sync_uploads (
    upload_id   UUID PRIMARY KEY,
    doc_id      BIGINT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    state       TEXT NOT NULL DEFAULT 'queued'
                CHECK (state IN ('queued', 'processing', 'applied', 'conflict', 'failed')),
    revision    BIGINT,               -- applied/conflict 시의 서버 revision
    patch       JSONB,                -- conflict 시 서버가 계산한 패치
    error       TEXT,
    created_at  BIGINT NOT NULL
);
CREATE INDEX IF NOT EXISTS sync_uploads_doc_idx
    ON sync_uploads (doc_id, created_at DESC);

-- ── v3: 스트로크 id는 문서별 공간 ───────────────────────────────────────────
-- 스냅샷의 획 id는 문서 내에서만 유일하면 충분합니다. 전역 PK를 (doc_id, id)로
-- 변경해 서로 다른 문서 간 id 충돌을 원천 차단합니다.
ALTER TABLE strokes DROP CONSTRAINT IF EXISTS strokes_pkey;
ALTER TABLE strokes ADD PRIMARY KEY (doc_id, id);
DROP INDEX IF EXISTS strokes_doc_id_idx;  -- PK가 같은 역할을 대신
