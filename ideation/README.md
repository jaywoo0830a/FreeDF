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
├─ src/idea4/         # 계약 객체 — 아키텍처의 공개 표면
│  ├─ events.js       # 통합 이벤트 어휘 (잎 — 의존성 0)
│  ├─ descriptor.js   # 장치 기술자 (버튼 개수/압력 등 = 데이터)
│  ├─ control-map.js  # 사용자 매핑 테이블 (tap/hold)
│  ├─ hub.js          # 이벤트 허브 (충돌 규칙 + 재생 계약)
│  ├─ devices.js      # 장치 어댑터 (번역 + 능력 협상)
│  ├─ tools.js        # 툴 상태기계 (참조 구현 3개)
│  ├─ workspace.js    # 정책의 집 (툴 선택·획 경계·hold 스택)
│  └─ index.js        # 배럴
└─ tests/
   ├─ sum.test.js     # 동작 확인용 샘플
   ├─ playground.test.js / idea*.test.js  # 아이디어별 프로토타입
   └─ idea4/          # 아키텍처 설계 테스트
      ├─ README.md              # 획 시작 에지 계약 — 참고 문서 (2026-09-15 회귀)
      ├─ layering.test.js       # 의존성 규칙 (정적 검사)
      ├─ contract.test.js       # 계약 객체 표면
      ├─ stroke-boundary.test.js# 획 경계 불변식 + 중첩 hold
      ├─ stroke-edge.test.js    # 획 시작 에지 계약 ("에지는 파괴되지 않는다")
      ├─ 0915-bug-case.test.js  # 교육용 스텁 — 0915 버그를 두 시계 미니 세계로 4막 재현
      ├─ session-router.test.js # 세션 라우터 — 땜질을 구조로 (실패하는 인터페이스 테스트 포함)
      ├─ canvas-interaction.test.js # 캔버스 상호작용 계층 (스텁)
      ├─ substitution.test.js   # 장치×툴 교체 매트릭스
      ├─ capability.test.js     # 가변 컨트롤 (0/2/5 버튼, 익스프레스 키)
      ├─ extensibility.test.js  # 툴 패키지 조립 (capability 요구)
      └─ invariants.js          # 잘-형성 커맨드 스트림 검사기 (공유)
```

## 동작 방식

- `test.sh`가 `tests/` 디렉터리와 `vitest.config.js`만 이미지의 `/app` 안으로
  **bind mount** 하므로, 호스트에서 테스트 파일을 수정하면 즉시 컨테이너에 반영됩니다.
- `node_modules`(vitest 포함)는 **이미지 레이어 안에만 존재**합니다. 호스트에는
  `node_modules`가 생기지 않고, 의존성 변경은 `package.json` 수정 후 재빌드
  (`docker build`는 `test.sh`가 매번 실행하지만 레이어 캐시로 즉시 끝남)로 반영됩니다.
- 컨테이너는 호스트와 같은 UID로 실행되고 캐시(`cacheDir`)는 컨테이너 `/tmp`에
  두므로, 생성되는 파일의 권한 문제가 없습니다.
