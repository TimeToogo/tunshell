FROM node:lts-slim

COPY server/ /app/server/
COPY shared/ /app/shared/

WORKDIR /app/shared

RUN npm ci --no-dev
RUN npm run build

WORKDIR /app/server

RUN npm ci --no-dev
RUN npm run build

ENTRYPOINT [ "node", "dist/main.js" ]