FROM node:lts-slim

COPY js/server/ /app/server/
COPY js/shared/ /app/shared/

WORKDIR /app/shared

RUN npm ci --no-dev
RUN npm run build

WORKDIR /app/server

RUN npm ci --no-dev
RUN npm run build

ENTRYPOINT [ "node", "dist/main.js" ]