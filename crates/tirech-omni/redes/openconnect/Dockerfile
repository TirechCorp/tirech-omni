FROM alpine:3
RUN apk add --no-cache openconnect iproute2 \
 && apk add --no-cache microsocks --repository=https://dl-cdn.alpinelinux.org/alpine/edge/testing
COPY entrypoint.sh /entrypoint.sh
ENTRYPOINT ["/entrypoint.sh"]
