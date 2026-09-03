-- FreeDF — 0006: 문서 전체 저장을 한 번의 왕복으로
--
-- 클라이언트가 획 배열(JSONB)·페이지 배열(JSONB)·PDF 바이트를 한 번에 보내면
-- 서버가 **하나의 문장**(자동으로 원자적)에서 전체를 반영합니다.
-- 왕복 수: 기존 4~5회(resync + pages + 문서 정보 + PDF) → **1회**.

CREATE OR REPLACE FUNCTION public.document_sync(
    p_doc_id      BIGINT,
    p_page_count  INT,
    p_strokes     JSONB,   -- [{id, page_index, tool, color, width, points, created_at}]
    p_pages       JSONB,   -- [{page_index, style, color, bookmarked}]
    p_pdf         BYTEA    -- NULL이면 PDF 본문 갱신 생략
) RETURNS void
LANGUAGE plpgsql
AS $$
BEGIN
    -- ① 스트로크 전체 재동기화
    DELETE FROM strokes WHERE doc_id = p_doc_id;
    INSERT INTO strokes (id, doc_id, page_index, tool, color, width, points, created_at)
    SELECT
        (s->>'id')::bigint,
        p_doc_id,
        (s->>'page_index')::int,
        s->>'tool',
        ARRAY(SELECT jsonb_array_elements_text(s->'color')::int),
        (s->>'width')::real,
        s->'points',
        (s->>'created_at')::bigint
    FROM jsonb_array_elements(p_strokes) s;

    -- ② 줄어든 페이지 삭제 (인덱스는 항상 0..N 연속 — 위치 기반)
    DELETE FROM pages
    WHERE doc_id = p_doc_id AND page_index >= p_page_count;

    -- ③ 용지/북마크 전체 upsert
    INSERT INTO pages (doc_id, page_index, style, color, bookmarked)
    SELECT
        p_doc_id,
        (p->>'page_index')::int,
        p->>'style',
        ARRAY(SELECT jsonb_array_elements_text(p->'color')::int),
        (p->>'bookmarked')::boolean
    FROM jsonb_array_elements(p_pages) p
    ON CONFLICT (doc_id, page_index) DO UPDATE SET
        style      = EXCLUDED.style,
        color      = EXCLUDED.color,
        bookmarked = EXCLUDED.bookmarked;

    -- ④ 문서 정보 + PDF 본문
    UPDATE documents
    SET page_count = p_page_count,
        pdf        = COALESCE(p_pdf, pdf),
        updated_at = (extract(epoch FROM clock_timestamp()) * 1000)::bigint
    WHERE id = p_doc_id;
END;
$$;
