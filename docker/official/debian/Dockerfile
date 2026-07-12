FROM debian:bookworm-slim

ENV DEBIAN_FRONTEND=noninteractive
ENV LANG=en_US.UTF-8
ENV LANGUAGE=en_US.UTF-8

RUN groupadd -r wyrd && useradd -r -g wyrd -s /bin/bash -d /app wyrd

RUN apt-get update --no-install-recommends \
  && apt-get install --no-install-recommends --yes \
    ca-certificates tzdata curl nginx supervisor netcat-openbsd gettext-base \
  && curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
  && apt-get install --no-install-recommends --yes nodejs \
  && rm -rf /var/lib/apt/lists/* \
  && apt-get autoremove \
  && apt-get clean

ARG NGINX_PORT=8000
ARG WYRD_SERVER_BINARY

ENV NGINX_PORT=${NGINX_PORT}

COPY --chown=wyrd:wyrd ${WYRD_SERVER_BINARY}/wyrd-server /usr/local/bin/wyrd-server
COPY --chown=wyrd:wyrd ${WYRD_SERVER_BINARY}/ui/node_modules /app/ui/node_modules/
COPY --chown=wyrd:wyrd ${WYRD_SERVER_BINARY}/ui/build /app/ui/build/
COPY --chown=wyrd:wyrd ${WYRD_SERVER_BINARY}/ui/package.json /app/ui/package.json
COPY --chown=wyrd:wyrd docker/official/extras/nginx/nginx.conf.template /etc/nginx/nginx.conf.template
COPY --chown=wyrd:wyrd docker/official/extras/entrypoint.sh /entrypoint.sh

RUN chown -R wyrd:wyrd /etc/nginx \
  && mkdir -p /var/log/nginx /var/run /var/lib/nginx \
  && rm -f /etc/nginx/conf.d/default.conf \
  && chmod +x /entrypoint.sh \
  && chmod +x /usr/local/bin/wyrd-server \
  && chown -R wyrd:wyrd /var/log/nginx /var/run /var/lib/nginx

WORKDIR /app

USER wyrd

EXPOSE ${NGINX_PORT}

CMD ["/entrypoint.sh"]
