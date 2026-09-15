# ideation — Node 24.21.0 + Vitest 5.0.0 테스트 샌드박스

호스트 환경과 무관하게 Docker 컨테이너 안에서 테스트 코드를 작성·실행할 수 있는
격리된 놀이터(ideation)입니다.

| 구성 | 버전 |
|---|---|
| Node.js | **24.21.0** (`node:24.21.0` 공식 이미지) |
| Vitest | **5.0.0** |

## 사용법

```bash
cd ideation

# 전체 테스트 실행 (이미지가 없으면 자동 빌드)
./test.sh

# 특정 테스트 파일만
./test.sh tests/sum.test.js

# 변경 감지(watch) 모드
./test.sh --watch          # 또는: ./test.sh run -- --watch
```

## 파일 구조

```
ideation/
├─ test.sh            # 컨테이너 빌드 + 테스트 실행 진입점
├─ Dockerfile         # node:24.21.0 + vitest@5.0.0
├─ package.json       # vitest 5.0.0 고정
├─ vitest.config.js   # tests/ 하위 *.test.{js,ts} 수집
└─ tests/             # 테스트 코드를 여기에 작성
   └─ sum.test.js
```

## 동작 방식

- `test.sh`가 `tests/` 디렉터리와 `vitest.config.js`만 이미지의 `/app` 안으로
  **bind mount** 하므로, 호스트에서 테스트 파일을 수정하면 즉시 컨테이너에 반영됩니다.
- `node_modules`(vitest 포함)는 **이미지 레이어 안에만 존재**합니다. 호스트에는
  `node_modules`가 생기지 않고, 의존성 변경은 `package.json` 수정 후 재빌드
  (`docker build`는 `test.sh`가 매번 실행하지만 레이어 캐시로 즉시 끝남)로 반영됩니다.
- 컨테이너는 호스트와 같은 UID로 실행되고 캐시(`cacheDir`)는 컨테이너 `/tmp`에
  두므로, 생성되는 파일의 권한 문제가 없습니다.
