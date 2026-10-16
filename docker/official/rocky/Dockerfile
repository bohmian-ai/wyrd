# The Wyrd server image on Rocky Linux: wyrd-server behind nginx. It
# copies the Linux bundle `scripts/build.sh` stages in release/ and
# compiles nothing:
#
#   scripts/build.sh image rocky wyrd:rocky
#
# Public ports: HTTP on 8080 and gRPC on 50051, both proxied by nginx to
# wyrd-server, which listens on loopback only.

FROM rockylinux:9-minimal
RUN microdnf install -y --setopt=install_weak_deps=0 \
    ca-certificates tzdata nginx gettext libatomic libstdc++ shadow-utils \
  && microdnf clean all \
  && groupadd -r wyrd && useradd -r -g wyrd -d /app wyrd

COPY release/wyrd-server /usr/local/bin/wyrd-server
COPY release/wyrd-monty-worker /usr/local/bin/wyrd-monty-worker
COPY docker/official/extras/nginx/nginx.conf.template /etc/nginx/nginx.conf.template
COPY docker/official/extras/entrypoint.sh /entrypoint.sh
RUN chmod 0755 /usr/local/bin/wyrd-server /usr/local/bin/wyrd-monty-worker /entrypoint.sh \
  && mkdir -p /var/lib/wyrd/storage /var/lib/wyrd/bifrost /var/lib/nginx /var/log/nginx \
  && chown -R wyrd:wyrd /etc/nginx /var/lib/wyrd /var/lib/nginx /var/log/nginx

# Mount /var/lib/wyrd persistently: local artifact storage and Scribe's
# private WAL/staging live there. Postgres and the signing key are external.
ENV WYRD_SERVER_BIND=127.0.0.1:8081 \
    WYRD_GRPC_BIND=127.0.0.1:50053 \
    WYRD_STORAGE_URL=file:///var/lib/wyrd/storage \
    WYRD_BIFROST_DATA_DIR=/var/lib/wyrd/bifrost
VOLUME ["/var/lib/wyrd"]
WORKDIR /app
USER wyrd
EXPOSE 8080 50051
CMD ["/entrypoint.sh"]
