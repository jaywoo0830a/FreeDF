-- FreeDF — 0004: 미디어 객체 메타데이터 (server/backend 용)
-- 파일 자체는 nginx가 디스크에서 직접 서빙(Range 스트리밍)하고,
-- 이 테이블은 메타데이터만 담습니다. doc_id는 참조만 유지 —
-- 문서가 삭제돼도 녹음 파일은 서버에 남습니다(미디어 패널에서 관리).
CREATE TABLE media_objects (
    id          BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    doc_id      BIGINT,               -- documents.id (nullable, 삭제 후에도 유지)
    kind        TEXT NOT NULL,        -- audio / image / ...
    name        TEXT NOT NULL,        -- 원본 파일명
    mime        TEXT NOT NULL,
    size        BIGINT NOT NULL,      -- 바이트
    object_key  TEXT NOT NULL UNIQUE, -- media/ 아래 상대 경로 (YYYY/MM/uuid.ext)
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX media_objects_doc_idx ON media_objects (doc_id, created_at DESC);
