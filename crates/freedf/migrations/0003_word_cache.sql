-- FreeDF v2 — 0003: 사전 캐시
-- 영단어 사전 오버레이의 조회 결과를 캐시합니다 (오프라인 재조회용).
CREATE TABLE word_cache (
    word        TEXT PRIMARY KEY,
    data        JSONB NOT NULL,
    updated_at  BIGINT NOT NULL
);
