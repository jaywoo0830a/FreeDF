-- FreeDF — 0003: 사전 캐시 (UNLOGGED)
-- 영단어 사전 오버레이의 조회 결과 캐시 — 재조회로 재구성 가능하므로
-- UNLOGGED로 WAL 쓰기를 생략합니다 (크래시 시 비워질 뿐).
CREATE UNLOGGED TABLE word_cache (
    word        TEXT PRIMARY KEY,
    data        JSONB NOT NULL,
    updated_at  BIGINT NOT NULL
);
