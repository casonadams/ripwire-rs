# Language servers for scripts/corpus/lsp2scip.py (the LSP bridge) and scripts/corpus/corpus.py. Built by
# `make corpus-image-lsp`. Companion to scripts/corpus/Dockerfile, which holds the SCIP indexers.
#
# linux/amd64 only (the Lua, Zig and Go archives below are x86_64 Linux builds). Every base image digest,
# package version and download checksum is pinned here: bump them together and re-run
# `make corpus-index FORCE=1 NAME=<entry>` for the affected entries. A language server here answers
# go-to-definition only; its answers are the oracle, so a version change can move the measured floors.

FROM node:22-trixie-slim@sha256:2117f713a417f3aff682ac8f5bc2ebbdf1beed51f36e67ab1e0fd3866b6d2d8f

# Debian trixie packages. clangd 19.0 (C, Objective-C, CUDA), Elixir 1.18.3 and OTP 27 (elixir-ls runs on them),
# xz for the Zig archive, python3 for the bridge and fortls, a JRE for the COBOL server, libfontconfig1 for Godot.
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      ca-certificates clangd=1:19.0-63 curl elixir=1.18.3.dfsg-1 git make python3 python3-pip python3-venv \
      unzip xz-utils libfontconfig1 openjdk-21-jre-headless \
 && rm -rf /var/lib/apt/lists/*

# Go 1.27.3 toolchain (gopls builds and runs on it); official archive, checksum from go.dev/dl.
ADD --checksum=sha256:65efbc80fdc1fa2cd2631cad5b481777a68e8725f34e89f50fc6ac714e379b40 \
    https://go.dev/dl/go1.27.3.linux-amd64.tar.gz /tmp/go.tar.gz
RUN tar -C /usr/local -xzf /tmp/go.tar.gz && rm /tmp/go.tar.gz
ENV GOROOT=/usr/local/go GOFLAGS=-modcacherw

# gopls v0.24.0 (Go): go install from the pinned module version, built with the toolchain above.
RUN GOBIN=/usr/local/bin /usr/local/go/bin/go install golang.org/x/tools/gopls@v0.24.0 \
 && rm -rf /root/go /root/.cache/go-build

# Zig 0.17.0 and ZLS 0.17.0 (Zig). ZLS needs the matching compiler for its standard-library paths.
ADD --checksum=sha256:1cbe9df9f27e6b78d14ccbca43b6703a404ef79ef1c463de901d7f088d4e2026 \
    https://ziglang.org/download/0.17.0/zig-x86_64-linux-0.17.0.tar.xz /tmp/zig.tar.xz
ADD --checksum=sha256:dfa4514e1868803fdf2ae56832c88b9d400094d70ea6bb136b57bcc91af10258 \
    https://github.com/zigtools/zls/releases/download/0.17.0/zls-x86_64-linux.tar.xz /tmp/zls.tar.xz
RUN mkdir -p /opt/zig /opt/zls \
 && tar -C /opt/zig --strip-components=1 -xJf /tmp/zig.tar.xz \
 && tar -C /opt/zls -xJf /tmp/zls.tar.xz \
 && rm /tmp/zig.tar.xz /tmp/zls.tar.xz \
 && ln -s /opt/zig/zig /usr/local/bin/zig && ln -s /opt/zls/zls /usr/local/bin/zls

# lua-language-server 3.19.2 (Lua).
ADD --checksum=sha256:93ad326b10f6c00911dd8c450fa098332dbd8f9f45d1052f5636a2d54479b947 \
    https://github.com/LuaLS/lua-language-server/releases/download/3.19.2/lua-language-server-3.19.2-linux-x64.tar.gz /tmp/lua.tar.gz
RUN mkdir -p /opt/lua-ls && tar -C /opt/lua-ls -xzf /tmp/lua.tar.gz && rm /tmp/lua.tar.gz \
 && ln -s /opt/lua-ls/bin/lua-language-server /usr/local/bin/lua-language-server

# elixir-ls v0.31.1 (Elixir): release archive; its language_server.sh runs on the Elixir and OTP above.
ADD --checksum=sha256:bac08322ea3698157eb2373bb5b65e38c15df9dd41e1c06f142f874367fa472f \
    https://github.com/elixir-lsp/elixir-ls/releases/download/v0.31.1/elixir-ls-v0.31.1.zip /tmp/elixir-ls.zip
RUN mkdir -p /opt/elixir-ls && unzip -q /tmp/elixir-ls.zip -d /opt/elixir-ls && rm /tmp/elixir-ls.zip \
 && chmod 0755 /opt/elixir-ls/language_server.sh

# fortls 3.2.2 (Fortran), in a virtualenv of its own.
RUN python3 -m venv /opt/fortls && /opt/fortls/bin/pip install --no-cache-dir fortls==3.2.2

# Node language servers: bash-language-server 5.8.1 (Bash), @nomicfoundation/solidity-language-server 0.9.3 (Solidity)
# and brighterscript 0.73.8 (BrightScript; `bsc --lsp` serves LSP over stdio).
RUN npm install -g bash-language-server@5.8.1 @nomicfoundation/solidity-language-server@0.9.3 brighterscript@0.73.8 \
 && npm cache clean --force

# COBOL language server 2.5.1 (COBOL): the server.jar of the VS Code extension, run on the JRE above. Launched with
# no arguments it serves LSP over stdio. The package's native binary has no LSP mode, so only the jar is kept.
ADD --checksum=sha256:53ee5701e666d117c890a7e694a307d0366d333fbde57d5ae06b47fc789fbfdc \
    https://github.com/eclipse-che4z/che-che4z-lsp-for-cobol/releases/download/2.5.1/cobol-language-support-linux-x64-2.5.1.vsix /tmp/cobol.vsix
RUN mkdir -p /opt/cobol-ls && unzip -q /tmp/cobol.vsix 'extension/server/jar/*' -d /opt/cobol-ls && rm /tmp/cobol.vsix \
 && printf '#!/bin/sh\nexec java -jar /opt/cobol-ls/extension/server/jar/server.jar "$@"\n' > /usr/local/bin/cobol-language-server \
 && chmod 0755 /usr/local/bin/cobol-language-server

# Godot 4.7.2 (GDScript): the editor serves the GDScript language server on TCP 6005 when run headless.
ADD --checksum=sha256:cadd3204e728a35d3f13adb7fd0d7902636b79f6b95c40c265eb73b6c35329e4 \
    https://github.com/godotengine/godot/releases/download/4.7.2-stable/Godot_v4.7.2-stable_linux.x86_64.zip /tmp/godot.zip
RUN mkdir -p /opt/godot && unzip -q /tmp/godot.zip -d /opt/godot && rm /tmp/godot.zip \
 && ln -s /opt/godot/Godot_v4.7.2-stable_linux.x86_64 /usr/local/bin/godot \
 && chmod 0755 /opt/godot/Godot_v4.7.2-stable_linux.x86_64

# The bridge itself (stdlib only): scripts/corpus/lsp2scip.py.
COPY lsp2scip.py /usr/local/lib/ripwire/lsp2scip.py

ENV GOPATH=/corpus-home/go GOMODCACHE=/corpus-home/gomod GOCACHE=/corpus-home/gocache \
    PATH=/opt/fortls/bin:/opt/zig:/usr/local/go/bin:/usr/local/bin:$PATH

WORKDIR /src
