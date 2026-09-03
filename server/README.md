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
- FreeDF의 획/세션/문서는 기존처럼 Postgres 직접 연결 (이 서버와 무관)

## 배포

```bash
sudo mkdir -p /srv/freedf-server/media
# 이 저장소의 server/ 내용을 /srv/freedf-server 에 복사 후
cd /srv/freedf-server
# 환경값 수정: docker-compose.yml 의 DATABASE_URL / PUBLIC_BASE_URL / FREEDF_API_KEY
docker compose up -d --build
```

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
