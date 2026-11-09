# aw-server from the static binary (python3 tools/build/server-image.py
# builds both — the binary and a ready image archive):
#   docker build -t aw-server -f source/apps/server/Dockerfile programs/server-linux-x86_64
FROM scratch
COPY aw-server /usr/local/bin/aw-server
ENV AW_ADMIN= AW_NAME= AW_PORT=7447
VOLUME /data
WORKDIR /data
EXPOSE 7447/udp 7447/tcp
ENTRYPOINT ["/usr/local/bin/aw-server"]
