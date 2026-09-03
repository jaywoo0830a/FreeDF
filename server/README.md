# FreeDF 미디어 리소스 서버 (자체 호스팅)

외부 서비스 의존 없이 VPS에서 직접 운영하는 미디어(음성 녹음 등) 스토리지 서버입니다.

## 구조

```
인터넷 → nginx(443) ── /media/* ──► VPS 디스크 직접 서빙 (Range 스트리밍 지원)
                     └─ /api/*  ──► backend(axum) ──► PostgreSQL (메타데이터)
```

- **다운로드/재생**은 nginx가 파일을 직접 서빙 → 오디오 앞/뒤 탐색(Range)이 무료로 동작
- **업로드/목록/삭제**만 백엔드 API 경유
- 메타데이터는 FreeDF와 **같은 PostgreSQL**의 `media_objects` 테이블 사용
  (스키마는 `server/db/up.sh` 마이그레이션 `0004_media_objects.sql`이 생성)
- FreeDF의 획/세션/문서는 기존처럼 Postgres 직접 연결 (이 서버와 무관)

## 배포

```bash
sudo mkdir -p /srv/freedf-server/media
# 이 저장소의 server/ 내용을 /srv/freedf-server 에 복사 후
cd /srv/freedf-server

# 1) DB (PostgreSQL 18.6 + 스키마)
cd db && ./init.sh && ./up.sh && cd ..

# 2) 백엔드 (업로드/목록/삭제 API)
cd backend && ./init.sh && ./up.sh && cd ..

# 3) nginx (정적 서빙 + API 프록시)
docker compose up -d nginx
```

- `db/init.sh` → `db/.env` 생성 (비밀번호 자동 생성).
  - **원격 접속(FreeDF 앱이 VPS 밖에서 직접 연결)**: `db/.env`의
    `FREEDF_DB_BIND=0.0.0.0` + `FREEDF_DB_HOST=<VPS IP/도메인>`으로 설정 후
    `db/up.sh` 재실행. 방화벽으로 앱 IP만 허용 권장: `sudo ufw allow from <앱IP> to any port 5432 proto tcp`
- `backend/init.sh` → `backend/.env` 생성 (API 키 자동 생성, FreeDF 앱 server.json에 입력).
  **같은 호스트의 `db/.env`를 자동으로 읽어 `DATABASE_URL`을 조립**하므로
  DB 비밀번호를 직접 복사할 필요가 없습니다.
- `db/up.sh`는 PostgreSQL 18.6 기동 후 `migrations/`를 순서대로 적용
- `backend/up.sh`는 backend 컨테이너만 빌드/기동, `backend/down.sh`로 중지
- PostgreSQL 튜닝: `db/postgresql.conf` (SSD 전용 — PG18 내장 비동기 I/O, WAL zstd 등)

> **Docker Desktop(Win/Mac)에서 개발할 때**: `network_mode: host`는 Docker
> VM 내부 네트워크라 호스트의 `127.0.0.1`과 다릅니다. 백엔드를 직접 테스트하려면
> `BIND=0.0.0.0:8080`으로 바꾸고 VM IP로 접근하거나, 컨테이너 없이
> `cargo run`으로 띄우세요. Ubuntu VPS(Docker Engine)에서는 호스트 네트워크가
> 그대로 동작해 `127.0.0.1` 바인딩이 맞습니다.

필수 환경 변수 (backend):
| 변수 | 기본값 | 설명 |
|---|---|---|
| `DATABASE_URL` | `postgres://freedf:freedf@localhost:5432/freedf` | FreeDF와 같은 Postgres |
| `MEDIA_DIR` | `/srv/freedf-server/media` | 파일 저장 경로 (nginx도 같은 경로를 읽음) |
| `PUBLIC_BASE_URL` | `https://media.example.com` | 클라이언트에 알려줄 공개 URL (응답의 `url` 필드 생성용) |
| `FREEDF_API_KEY` | (필수, 없으면 기동 거부) | API 호출 시 `X-Api-Key` 헤더 |
| `BIND` | `127.0.0.1:8080` | 백엔드 바인딩 주소 |

## API

모든 호출에 `X-Api-Key: <FREEDF_API_KEY>` 헤더 필요.

| 메서드 | 경로 | 설명 |
|---|---|---|
| GET | `/health` | 상태 확인 |
| POST | `/api/media?doc_id=1&kind=audio` | multipart `file` 업로드 → JSON 메타데이터 반환 |
| GET | `/api/media?doc_id=1&limit=50&offset=0` | 목록 (최신순) |
| DELETE | `/api/media/:id` | 파일+행 삭제 |

## TLS

- Let's Encrypt: `sudo certbot certonly --webroot -w /var/www/certbot -d media.example.com` 후 `certs/`에 연결 (기본값은 자체 서명 예시)
- 자체 서명으로 시작하려면: `openssl req -x509 -nodes -days 365 -newkey rsa:2048 -keyout certs/key.pem -out certs/cert.pem -subj "/CN=media.example.com"`

## FreeDF 클라이언트 연동 (예정)

- 설정에 `media_base_url` + `api_key` 추가 → 녹음 업로드/재생 시 이 API 사용
