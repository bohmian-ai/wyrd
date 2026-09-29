# syntax=docker/dockerfile:1
#
# The one Wyrd application image: Rust server, Node BFF, and nginx.
#
# The server binary is built outside this file (release CI, `mise run
# docker:build`, or the startup lane) and staged at `binary/wyrd-server` in
# the repository build context:
#
#   docker build -f docker/official/Dockerfile -t wyrd .
#
# Public ports: HTTP on 8080 (API routes to Rust, everything else to the BFF)
# and gRPC on 50051. Rust and the BFF listen on loopback only.

FROM node:26.2.0-slim AS ui
RUN npm install --global pnpm@11.4.0
WORKDIR /src/crates/wyrd/wyrd-server/wyrd-ui
COPY crates/wyrd/wyrd-server/wyrd-ui/package.json crates/wyrd/wyrd-server/wyrd-ui/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY crates/wyrd-spec/schemas /src/crates/wyrd-spec/schemas
COPY crates/wyrd/wyrd-server/wyrd-ui ./
RUN pnpm build && pnpm prune --prod

FROM ubuntu:24.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
  && apt-get install --no-install-recommends --yes ca-certificates tzdata nginx gettext-base libatomic1 \
  && rm -rf /var/lib/apt/lists/* \
  && groupadd -r wyrd && useradd -r -g wyrd -d /app wyrd

COPY --from=ui /usr/local/bin/node /usr/local/bin/node
COPY --from=ui --chown=wyrd:wyrd /src/crates/wyrd/wyrd-server/wyrd-ui/build /app/ui/build
COPY --from=ui --chown=wyrd:wyrd /src/crates/wyrd/wyrd-server/wyrd-ui/node_modules /app/ui/node_modules
COPY --from=ui --chown=wyrd:wyrd /src/crates/wyrd/wyrd-server/wyrd-ui/package.json /app/ui/package.json
COPY binary/wyrd-server /usr/local/bin/wyrd-server
COPY docker/official/extras/nginx/nginx.conf.template /etc/nginx/nginx.conf.template
COPY docker/official/extras/entrypoint.sh /entrypoint.sh
RUN chmod 0755 /usr/local/bin/wyrd-server /entrypoint.sh

RUN mkdir -p /var/lib/wyrd/storage /var/lib/wyrd/bifrost /var/lib/nginx /var/log/nginx \
  && rm -f /etc/nginx/sites-enabled/default \
  && chown -R wyrd:wyrd /etc/nginx /var/lib/wyrd /var/lib/nginx /var/log/nginx

# Mount /var/lib/wyrd persistently: local artifact storage and Scribe's
# private WAL/staging live there. Postgres and the signing key are external.
ENV WYRD_SERVER_BIND=127.0.0.1:8081 \
    WYRD_GRPC_BIND=127.0.0.1:50053 \
    WYRD_BIFROST_DATA_DIR=/var/lib/wyrd/bifrost
VOLUME ["/var/lib/wyrd"]
WORKDIR /app
USER wyrd
EXPOSE 8080 50051
CMD ["/entrypoint.sh"]
