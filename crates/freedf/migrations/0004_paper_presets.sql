-- PagePaper 축소: 줄/격자/점 세부설정(간격/색/두께)은 페이지별 저장을
-- 없애고 스타일별 프리셋(세션 상태)으로 이동합니다.
ALTER TABLE pages DROP COLUMN IF EXISTS spacing;
ALTER TABLE pages DROP COLUMN IF EXISTS line_color;
ALTER TABLE pages DROP COLUMN IF EXISTS line_width;
