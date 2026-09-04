-- FreeDF — 0009: 미디어를 페이지 단위로 (doc_id + page_index)
-- page_index는 0 기반 페이지 번호. NULL이면 문서 전체 공유 미디어.
ALTER TABLE media_objects ADD COLUMN page_index integer;
CREATE INDEX media_objects_doc_page_idx ON media_objects (doc_id, page_index);
