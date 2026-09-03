-- FreeDF — 0005: DB 인스턴스 식별자 (로컬 캐시 무효화용)
-- DB를 초기화(initdb)하면 이 UUID가 새로 생성되고, 클라이언트는 식별자가
-- 달라진 것을 보고 로컬 캐시 전체를 폐기합니다 — 문서 id가 재사용되면서
-- 옛 스트로크/대기열/목록이 새 문서에 섞이는 오염을 방지합니다.
CREATE TABLE db_identity (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    uuid      TEXT NOT NULL
);
INSERT INTO db_identity (uuid) VALUES (gen_random_uuid());
