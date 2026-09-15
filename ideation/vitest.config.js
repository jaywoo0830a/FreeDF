import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    // ideation 폴더의 테스트만 수집
    include: ['tests/**/*.test.{js,ts,jsx,tsx}'],
    // 컨테이너 안에서 실행되므로 캐시는 컨테이너 로컬(/tmp)에 둔다.
    // bind mount + named volume 조합에서 호스트 파일 권한 문제를 피하기 위함.
    cacheDir: '/tmp/vitest-cache',
  },
})
