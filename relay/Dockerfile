FROM node:lts-slim

WORKDIR /app/

COPY . /app/

RUN npm ci --no-dev
RUN npm run build


ENTRYPOINT [ "node", "dist/main.js" ]