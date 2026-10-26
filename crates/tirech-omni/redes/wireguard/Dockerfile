FROM alpine:3
# O microsocks só está no repositório testing do Alpine.
RUN apk add --no-cache wireguard-tools iproute2 iptables \
 && apk add --no-cache microsocks --repository=https://dl-cdn.alpinelinux.org/alpine/edge/testing
COPY entrypoint.sh /entrypoint.sh
ENTRYPOINT ["/entrypoint.sh"]
