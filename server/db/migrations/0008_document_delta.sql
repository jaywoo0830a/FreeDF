-- FreeDF — 0008: 델타 동기화 함수
--
-- 전체 스트로크를 재전송하지 않고 구조 연산을 서버에서 처리합니다.
-- 프로토콜: 앱은 획을 write-behind로 이미 증분 전송하므로, 구조 연산은
--  1) 대기열 플러시 → 2) 서버 델타(인덱스 이동/삭제/회전) → 3) 메타 동기화
-- 순서로 실행합니다 (전체 resync는 repair용 document_sync(0006)만 남김).

-- ── 메타 동기화: 페이지(용지/북마크)·문서 정보·PDF. 스트로크는 건드리지 않음 ──
CREATE OR REPLACE FUNCTION public.document_sync_meta(
    p_doc_id      BIGINT,
    p_page_count  INT,
    p_pages       JSONB,   -- [{page_index, style, color, bookmarked}]
    p_pdf         BYTEA    -- NULL이면 PDF 본문 갱신 생략
) RETURNS void
LANGUAGE plpgsql
AS $$
BEGIN
    DELETE FROM pages
    WHERE doc_id = p_doc_id AND page_index >= p_page_count;

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

    UPDATE documents
    SET page_count = p_page_count,
        pdf        = COALESCE(p_pdf, pdf),
        updated_at = (extract(epoch FROM clock_timestamp()) * 1000)::bigint
    WHERE id = p_doc_id;
END;
$$;

-- ── 페이지 중간 삽입: from 이상 획의 page_index를 delta만큼 이동 ──
CREATE OR REPLACE FUNCTION public.document_shift_strokes(
    p_doc_id BIGINT,
    p_from   INT,
    p_delta  INT
) RETURNS void
LANGUAGE sql
AS $$
    UPDATE strokes SET page_index = page_index + p_delta
    WHERE doc_id = p_doc_id AND page_index >= p_from
$$;

-- ── 페이지 삭제: 해당 페이지 획 삭제 + 이후 인덱스 -1 ──
CREATE OR REPLACE FUNCTION public.document_delete_page(
    p_doc_id BIGINT,
    p_page   INT
) RETURNS void
LANGUAGE plpgsql
AS $$
BEGIN
    DELETE FROM strokes WHERE doc_id = p_doc_id AND page_index = p_page;
    UPDATE strokes SET page_index = page_index - 1
    WHERE doc_id = p_doc_id AND page_index > p_page;
END;
$$;

-- ── 페이지 회전: 해당 페이지 획의 x/y를 서버에서 변환 (재전송 없음) ──
-- 앱과 동일한 변환: 시계방향 (x,y) → (h-y, x), 반시계 (x,y) → (y, w-x)
CREATE OR REPLACE FUNCTION public.document_rotate_page(
    p_doc_id    BIGINT,
    p_page      INT,
    p_clockwise BOOL,
    p_w         REAL,
    p_h         REAL
) RETURNS void
LANGUAGE plpgsql
AS $$
BEGIN
    IF p_clockwise THEN
        UPDATE strokes
        SET points = (
            SELECT jsonb_agg(
                jsonb_set(
                    jsonb_set(pt, '{x}', to_jsonb(p_h - (pt->>'y')::float8)),
                    '{y}',
                    to_jsonb((pt->>'x')::float8)
                )
                ORDER BY ord
            )
            FROM jsonb_array_elements(points) WITH ORDINALITY AS e(pt, ord)
        )
        WHERE doc_id = p_doc_id AND page_index = p_page;
    ELSE
        UPDATE strokes
        SET points = (
            SELECT jsonb_agg(
                jsonb_set(
                    jsonb_set(pt, '{x}', to_jsonb((pt->>'y')::float8)),
                    '{y}',
                    to_jsonb(p_w - (pt->>'x')::float8)
                )
                ORDER BY ord
            )
            FROM jsonb_array_elements(points) WITH ORDINALITY AS e(pt, ord)
        )
        WHERE doc_id = p_doc_id AND page_index = p_page;
    END IF;
END;
$$;

-- ── 전체 페이지 회전: 페이지별 크기([w,h] 배열)를 받아 내부에서 반복 ──
CREATE OR REPLACE FUNCTION public.document_rotate_all(
    p_doc_id    BIGINT,
    p_clockwise BOOL,
    p_sizes     JSONB    -- [[w0,h0],[w1,h1],...] (page_index 순서)
) RETURNS void
LANGUAGE plpgsql
AS $$
DECLARE
    s jsonb;
    i int := 0;
    w real;
    h real;
BEGIN
    FOR s IN SELECT value FROM jsonb_array_elements(p_sizes) LOOP
        w := (s->>0)::real;
        h := (s->>1)::real;
        PERFORM public.document_rotate_page(p_doc_id, i, p_clockwise, w, h);
        i := i + 1;
    END LOOP;
END;
$$;
