# ZOC's pinned build and test image.
#
# The image holds the toolchains and the Linux desktop runtime needed to:
#   - build the official OpenCode Web UI,
#   - compile and package ZOC with MyGo,
#   - run Go tests, adapter tests and a WebKitGTK smoke test under Xvfb.
#
# Toolchains are downloaded and checksum-verified rather than taken from the
# distribution repositories, so the Go and Bun versions do not drift with
# Fedora updates. The base image itself is pinned by digest.
FROM registry.fedoraproject.org/fedora@sha256:a30417bd6c285eace1605deb9fba1160cbe2606f144ee3663bc4e65ec30c5da2

ARG GO_VERSION=1.27.4
ARG GO_SHA256=7226736c2339719cda28fbf3a54d71aa63bcefe376e98cef00034268dab24840
ARG BUN_VERSION=1.4.4
ARG BUN_SHA256=bf8ccbfb2b0b80159e0f9a6d15c55317add8077534dbc6130a3edb52fccb395c

LABEL org.opencontainers.image.title="zoc-build" \
      org.opencontainers.image.description="Pinned Fedora build/test image for ZOC" \
      org.opencontainers.image.version="2026-11-16" \
      dev.zoc.go.version="${GO_VERSION}" \
      dev.zoc.bun.version="${BUN_VERSION}"

RUN dnf install -y --setopt=install_weak_deps=False \
      ca-certificates \
      curl \
      git \
      tar \
      gzip \
      xz \
      unzip \
      findutils \
      procps-ng \
      jq \
      python3 \
      openssl \
      desktop-file-utils \
      gtk3 \
      webkit2gtk4.1 \
      xdg-dbus-proxy \
      bubblewrap \
      libnotify \
      dbus-daemon \
      xorg-x11-server-Xvfb \
      xauth \
      mesa-libGL \
      mesa-dri-drivers \
      fontconfig \
      dejavu-sans-fonts \
      dejavu-sans-mono-fonts \
      google-noto-sans-cjk-ttc-fonts \
      rpm \
    && dnf clean all \
    && rm -rf /var/cache/dnf

RUN curl -fsSL "https://go.dev/dl/go${GO_VERSION}.linux-amd64.tar.gz" -o /tmp/go.tar.gz \
    && echo "${GO_SHA256}  /tmp/go.tar.gz" | sha256sum -c - \
    && tar -C /usr/local -xzf /tmp/go.tar.gz \
    && rm /tmp/go.tar.gz \
    && /usr/local/go/bin/go version

RUN curl -fsSL "https://github.com/oven-sh/bun/releases/download/bun-v${BUN_VERSION}/bun-linux-x64.zip" -o /tmp/bun.zip \
    && echo "${BUN_SHA256}  /tmp/bun.zip" | sha256sum -c - \
    && unzip -q /tmp/bun.zip -d /opt \
    && mv /opt/bun-linux-x64 /opt/bun \
    && rm /tmp/bun.zip \
    && ln -s /opt/bun/bun /usr/local/bin/bun \
    && bun --version

ENV PATH=/usr/local/go/bin:/usr/local/bin:/usr/bin:/bin \
    GOTOOLCHAIN=local \
    CGO_ENABLED=0 \
    GOFLAGS=-mod=readonly \
    HOME=/tmp/zoc-home \
    XDG_CACHE_HOME=/tmp/zoc-cache \
    XDG_CONFIG_HOME=/tmp/zoc-config \
    XDG_DATA_HOME=/tmp/zoc-data \
    ELECTRON_RUN_AS_NODE= \
    WEBKIT_DISABLE_DMABUF_RENDERER=1 \
    LIBGL_ALWAYS_SOFTWARE=1 \
    NO_AT_BRIDGE=1

RUN mkdir -p /tmp/zoc-home /tmp/zoc-cache /tmp/zoc-config /tmp/zoc-data /src \
    && chmod 1777 /tmp/zoc-home /tmp/zoc-cache /tmp/zoc-config /tmp/zoc-data

WORKDIR /src
