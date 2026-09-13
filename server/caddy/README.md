# FreeDF — Caddy(호스트 웹 서버) 설정 가이드

Caddy는 80/443(TLS 종료 + 자동 HTTPS)을 받아 내부의 nginx(8081)로
포워딩하고, **HTTP/3(QUIC)** 까지 함께 서빙하는 최전방 소프트웨어입니다.

```
인터넷 ──443/TCP (HTTP/2·HTTP/1.1)──▶ Caddy ──reverse_proxy──▶ nginx(8081) ──▶ backend(8080)
      └─443/UDP (HTTP/3 / QUIC)──▶  (같은 Caddy)
```

## 배포

```bash
# 1) 호스트에 Caddy 설치 (Ubuntu 공식 apt 저장소)
sudo apt-get update && sudo apt-get install -y caddy

# 2) 설정 파일 배치 (도메인/포트 확인 후)
sudo cp server/caddy/Caddyfile /etc/caddy/Caddyfile
sudo systemctl reload caddy

# 3) 방화벽 — HTTP/3를 위해 UDP 443 도 반드시 열기
sudo ufw allow 80,443/tcp
sudo ufw allow 443/udp
```

## HTTP/3 활성 조건

- Caddy **2.8+** 는 TLS(자동 HTTPS)가 켜진 `server` 블록에서 **별도 지시어
  없이** HTTP/3(QUIC) 라우터를 443/UDP 에 자동으로 띄웁니다. (이 프로젝트는
  `caddy:2` ≥ 2.8 을 전제로 합니다.)
- 포트: HTTP/3은 **UDP 443** 이므로 방화벽에서 `443/udp` 를 반드시 열어야
  합니다 (위 ufw 명령 참고). TCP 443/80 은 HTTP/2·HTTP/1.1 폴백용.
- 커널은 이미 최적화되어 있으므로 QUIC 사이즈/백로그에 추가로 건드릴 것은 없습니다.
- QUIC이 차단된 네트워크(일부 기업망)에서는 브라우저가 자동으로 HTTP/2로
  폴백하므로 서비스는 항상 동작합니다 (공개 URL은 동일).

## 1Gbps 튜닝 포인트 (Caddyfile `freedf.*.online` 블록)

| 설정 | 값 | 이유 |
|---|---|---|
| HTTP/3(QUIC) | 자동(Caddy 2.8+) | 멀티스트리밍·0-RTT — 동시 전송 최적화 |
| `reverse_proxy localhost:8081` | — | nginx·backend에 자동으로 X-Forwarded-* 전달 |
| `Strict-Transport-Security` | 1년 | HTTPS 강제 — 이후 TLS 오버헤드 절감 |
| 본문 크기 제한 | 없음(Caddy 기본) | 200MB 미디어/스냅샷 그대로 통과 |
| 압축 | Caddy 기본(`encode`) | 텍스트만 zstd/gzip — 미디어는 이미 압축돼 있어 제외 |

> 도메인 변경: `Caddyfile`의 사이트명, `server/nginx/freedf.conf`의 `server_name`,
> `server/backend/.env`의 `PUBLIC_BASE_URL` 세 곳을 함께 수정하세요.