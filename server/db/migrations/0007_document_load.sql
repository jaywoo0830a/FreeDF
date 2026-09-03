-- FreeDF — 0007: 문서 로드를 한 번의 왕복으로
--
-- 주석(획 전체)·페이지·편집 저널·세션을 서버가 JSONB 배열로 집계해 반환 —
-- 클라이언트는 왕복 1회 + 단일 패스 serde 파싱으로 로드합니다.
-- (기존: 스트로크/페이지/저널/세션 쿼리 4회 + 획마다 개별 JSON 파싱)

CREATE OR REPLACE FUNCTION public.document_load(p_doc_id BIGINT)
RETURNS TABLE(strokes JSONB, pages JSONB, edits JSONB, session JSONB)
LANGUAGE sql STABLE
AS $$
    SELECT
        COALESCE((
            SELECT jsonb_agg(s ORDER BY s.id)
            FROM (
                SELECT id, page_index, tool, color, width, points, created_at
                FROM strokes WHERE doc_id = p_doc_id
            ) s
        ), '[]'::jsonb),
        COALESCE((
            SELECT jsonb_agg(p ORDER BY p.page_index)
            FROM (
                SELECT page_index, style, color, bookmarked
                FROM pages WHERE doc_id = p_doc_id
            ) p
        ), '[]'::jsonb),
        COALESCE((
            SELECT jsonb_agg(e.edit ORDER BY e.id)
            FROM doc_edits e WHERE e.doc_id = p_doc_id
        ), '[]'::jsonb),
        (SELECT state FROM sessions WHERE doc_id = p_doc_id)
$$;
