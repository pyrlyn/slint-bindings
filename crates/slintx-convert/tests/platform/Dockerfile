# slintx-convert's Linux platform tests: Blueprint through blueprint-compiler
# and GTK 4, HTML through headless Chromium, both on an X server from xvfb.
# Debian, because Fedora's blueprint-compiler does not find the GTK 4
# typelibs. `just convert-platforms` runs it on Colima; CI runs it on Linux.
# Run it with `docker run --init`: as PID 1, xvfb-run never sees Xvfb start.
FROM rust:1.99.1-slim-trixie@sha256:1a939748a7cbab172de91a014e3804a97af1394a8c844faa67fce9401c4e40fb

# libfontconfig1-dev and pkg-config build the Slint reference (dev-only).
# DejaVu is the one font, so Slint, GTK and Chromium measure the same glyphs.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        blueprint-compiler \
        chromium \
        curl \
        fonts-dejavu-core \
        gir1.2-gtk-4.0 \
        libfontconfig1-dev \
        pkg-config \
        python3-gi \
        xauth \
        xvfb \
    && rm -rf /var/lib/apt/lists/*

# The repo's test runner, at mise.toml's version. Slint takes one platform
# per process, so the geometry tests need a process per test.
ARG NEXTEST=0.9.143
RUN case "$(uname -m)" in aarch64) os=linux-arm ;; *) os=linux ;; esac \
    && curl -LsSf "https://get.nexte.st/${NEXTEST}/${os}" | tar zxf - -C /usr/local/cargo/bin

# Software rendering and no accessibility bus: the same frames on every host.
# No bytecode caches in the mounted checkout.
ENV GSK_RENDERER=cairo GDK_BACKEND=x11 NO_AT_BRIDGE=1 LIBGL_ALWAYS_SOFTWARE=1 PYTHONDONTWRITEBYTECODE=1

WORKDIR /src
CMD ["xvfb-run", "-a", "-s", "-screen 0 1280x1024x24", "cargo", "nextest", "run", "--locked", "-p", "slintx-convert", "--run-ignored", "all"]
