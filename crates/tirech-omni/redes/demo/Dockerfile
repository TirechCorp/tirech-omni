FROM alpine:3
RUN apk add --no-cache openssh-server iproute2 \
 && apk add --no-cache microsocks --repository=https://dl-cdn.alpinelinux.org/alpine/edge/testing \
 && adduser -D demo && echo "demo:demo" | chpasswd
COPY entrypoint.sh /entrypoint.sh
ENTRYPOINT ["/entrypoint.sh"]
