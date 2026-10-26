FROM alpine:3
RUN apk add --no-cache ppp iproute2 \
 && apk add --no-cache microsocks openfortivpn --repository=https://dl-cdn.alpinelinux.org/alpine/edge/testing
COPY entrypoint.sh /entrypoint.sh
ENTRYPOINT ["/entrypoint.sh"]
