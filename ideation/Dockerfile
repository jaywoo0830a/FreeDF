FROM node:24.21.0

WORKDIR /app

# vitest 5.0.0 고정 설치 (node:24.21.0 + vitest 5.0.0 환경을 이미지 레이어에 스냅샷)
COPY package.json ./
RUN npm install --no-audit --no-fund

# vite 가 설정 파일 로드 시 /app 에 임시 번들 파일을 쓰므로
# 임의의 호스트 UID 로 실행해도 쓸 수 있게 디렉터리를 연다.
# (node_modules 는 읽기만 하면 되고, 쓰기는 컨테이너 로컬에 한정됨)
RUN chown node:node /app && chmod 777 /app

# 기본 명령은 test.sh 가 결정 (여기서는 vitest run)
CMD ["npx", "vitest", "run"]
