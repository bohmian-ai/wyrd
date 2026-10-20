# The Wyrd application image on Rocky Linux (amd64 only): wyrd-server, the
# Node BFF, and nginx. It copies the Linux bundle `scripts/build.sh` stages
# in release/ and compiles nothing:
#
#   scripts/build.sh image rocky wyrd:rocky
#
# Public ports: HTTP on 8080 (Rust routes to wyrd-server, everything else to
# the BFF) and gRPC on 50051. wyrd-server and the BFF listen on loopback only.

# Node's official glibc build runs on Rocky 9's newer glibc.
FROM node:26.2.0-bookworm-slim AS node

FROM rockylinux:9-minimal
RUN microdnf install -y --setopt=install_weak_deps=0 \
    ca-certificates tzdata nginx gettext libatomic libstdc++ shadow-utils \
  && microdnf clean all \
  && groupadd -r wyrd && useradd -r -g wyrd -d /app wyrd

COPY --from=node /usr/local/bin/node /usr/local/bin/node
COPY --chown=wyrd:wyrd release/ui /app/ui
COPY release/wyrd-server /usr/local/bin/wyrd-server
COPY docker/official/extras/nginx/nginx.conf.template /etc/nginx/nginx.conf.template
COPY docker/official/extras/entrypoint.sh /entrypoint.sh
# The image's Node loads the BFF and every runtime dependency it ships with.
RUN chmod 0755 /usr/local/bin/wyrd-server /entrypoint.sh \
  && cd /app/ui && node --input-type=module -e 'await import("./build/handler.js")' && cd / \
  && mkdir -p /var/lib/wyrd/storage /var/lib/wyrd/bifrost /var/lib/nginx /var/log/nginx \
  && chown -R wyrd:wyrd /etc/nginx /var/lib/wyrd /var/lib/nginx /var/log/nginx

# Mount /var/lib/wyrd persistently: local artifact storage and Scribe's
# private WAL/staging live there. Postgres and the signing key are external.
ENV WYRD_SERVER_BIND=127.0.0.1:8081 \
    WYRD_GRPC_BIND=127.0.0.1:50053 \
    WYRD_UI_BIND=127.0.0.1:3000 \
    WYRD_STORAGE_URL=file:///var/lib/wyrd/storage \
    WYRD_BIFROST_DATA_DIR=/var/lib/wyrd/bifrost
VOLUME ["/var/lib/wyrd"]
WORKDIR /app
USER wyrd
EXPOSE 8080 50051
CMD ["/entrypoint.sh"]
