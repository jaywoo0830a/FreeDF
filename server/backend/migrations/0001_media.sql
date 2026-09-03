-- FreeDF 미디어 메타데이터 (파일은 디스크, 행은 여기)
CREATE TABLE IF NOT EXISTS media_objects (
    id          BIGSERIAL PRIMARY KEY,
    doc_id      BIGINT,                 -- FreeDF documents.id (nullable)
    kind        TEXT NOT NULL,          -- audio / image / ...
    name        TEXT NOT NULL,          -- 원본 파일명
    mime        TEXT NOT NULL,
    size        BIGINT NOT NULL,        -- 바이트
    object_key  TEXT NOT NULL UNIQUE,   -- media/ 아래 상대 경로 (YYYY/MM/uuid.ext)
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS media_objects_doc_idx ON media_objects (doc_id, created_at DESC);
