# FreeDF 미디어 리소스 서버 (자체 호스팅)

외부 서비스 의존 없이 VPS에서 직접 운영하는 미디어(음성 녹음 등) 스토리지 서버입니다.

## 구조

```
인터넷 → Caddy(80/443, TLS) → nginx(8081) ── /media/* ──► 디스크 직접 서빙 (Range)
                                       └─ /v3/*, /api/* ──► backend(axum) ──► PostgreSQL
```

- **다운로드/재생**은 nginx가 파일을 직접 서빙 → 오디오 앞/뒤 탐색(Range)이 무료로 동작
- **업로드/목록/삭제**와 **Sync v3 동기화**(문서/획/세션 ZIP)는 backend API 경유
- 메타데이터·문서·획은 같은 PostgreSQL의 테이블 사용
  (스키마는 `server/db/up.sh` 마이그레이션이 생성)
- FreeDF 앱은 Postgres 직접 연결하지 않습니다 — 서버 주소(server.json)만 사용

## 배포

```bash
sudo mkdir -p /srv/freedf-server/media
# 이 저장소의 server/ 내용을 /srv/freedf-server 에 복사 후
cd /srv/freedf-server

# 1) DB (PostgreSQL 18.6 + 스키마)
cd db && ./init.sh && ./up.sh && cd ..

# 2) 백엔드 + nginx (Sync v3 + 미디어 API, 정적 서빙 + 프록시)
cd backend && ./init.sh && ./up.sh && cd ..
```

- `db/init.sh` → `db/.env` 생성 (비밀번호 자동 생성). DB는 backend만 접속 —
  backend가 다른 호스트일 때만 `FREEDF_DB_BIND=0.0.0.0` + `FREEDF_DB_HOST=<IP>`로 변경.
- `backend/init.sh` → `backend/.env` 생성 (API 키 자동 생성, FreeDF 앱 server.json에 입력).
  **같은 호스트의 `db/.env`를 자동으로 읽어 `DATABASE_URL`을 조립**하므로
  DB 비밀번호를 직접 복사할 필요가 없습니다.
- `db/up.sh`는 PostgreSQL 18.6 기동 후 `migrations/`를 순서대로 적용
- `backend/up.sh`는 backend+nginx 컨테이너를 빌드/기동, `backend/down.sh`로 중지
- PostgreSQL 튜닝: `db/postgresql.conf` (SSD 전용 — PG18 내장 비동기 I/O, WAL zstd 등)

> **Docker Desktop(Win/Mac)에서 개발할 때**: `network_mode: host`는 Docker
> VM 내부 네트워크라 호스트의 `127.0.0.1`과 다릅니다. 백엔드를 직접 테스트하려면
> `FREEDF_BACKEND_BIND=0.0.0.0 ./up.sh`로 게시하거나, 컨테이너 없이
> `cargo run`으로 띄우세요. Ubuntu VPS(Docker Engine)에서는 호스트 네트워크가
> 그대로 동작해 `127.0.0.1` 바인딩이 맞습니다.

필수 환경 변수 (backend):
| 변수 | 기본값 | 설명 |
|---|---|---|
| `DATABASE_URL` | `postgres://freedf:freedf@localhost:5432/freedf` | FreeDF와 같은 Postgres |
| `MEDIA_DIR` | `/srv/freedf-server/media` | 파일 저장 경로 (nginx도 같은 경로를 읽음) |
| `PUBLIC_BASE_URL` | `https://freedf.rlawjddn00.inline` | 클라이언트에 알려줄 공개 URL (응답의 `url` 필드 생성용) |
| `FREEDF_API_KEY` | (필수, 없으면 기동 거부) | API 호출 시 `X-Api-Key` 헤더 |
| `BIND` | `127.0.0.1:8080` | 백엔드 바인딩 주소 |

## API

모든 호출에 `X-Api-Key: <FREEDF_API_KEY>` 헤더 필요.
(Sync v3 엔드포인트는 `docs/sync-protocol-v3.md` / `docs/openapi/sync-v3.openapi.yaml`)

| 메서드 | 경로 | 설명 |
|---|---|---|
| GET | `/health` | 상태 확인 |
| POST | `/api/media?doc_id=1&kind=audio` | multipart `file` 업로드 → JSON 메타데이터 반환 |
| GET | `/api/media?doc_id=1&limit=50&offset=0` | 목록 (최신순) |
| DELETE | `/api/media/:id` | 파일+행 삭제 |

## TLS / 도메인

- TLS는 **Caddy**가 종료: 80/443을 받아 nginx(8081)로 포워딩.
  예: `freedf.rlawjddn00.inline { reverse_proxy localhost:8081 }` (자동 HTTPS)
- nginx는 8081에서 평문 HTTP (backend 127.0.0.1:8080 프록시 + /media 서빙).
- 도메인을 바꾸려면 `nginx/freedf.conf`의 `server_name`과
  `backend/.env`의 `PUBLIC_BASE_URL`을 함께 수정하세요.

## FreeDF 클라이언트 연동

- FreeDF 앱 첫 실행 대화상자(또는 Server 설정 창)에서
  `https://freedf.rlawjddn00.inline` + `backend/.env`의 API 키를 입력하면
  `server.json`에 저장됩니다 — Sync v3 동기화(/v3/*)와 녹음(/api/media)이
  같은 서버/키로 동작합니다.
