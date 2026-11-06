# Local SCIP indexers for scripts/corpus/corpus.py. Built by `make corpus-image`.
#
# linux/amd64 only: scip-clang publishes x86_64-linux and arm64-darwin binaries, not arm64-linux, so on
# Apple Silicon Docker Desktop runs this image under Rosetta. The image only INDEXES; ripwire-rs runs on
# the host against the checkouts and the .scip files it writes.
#
# Every base image digest, package version and download checksum is pinned below. Bump them together
# and re-run `make corpus-index FORCE=1` for the affected entries.

FROM node:22-trixie-slim@sha256:2117f713a417f3aff682ac8f5bc2ebbdf1beed51f36e67ab1e0fd3866b6d2d8f

# .NET SDK 8.0 (scip-dotnet runs on it), copied from Microsoft's pinned, Debian-based SDK image.
COPY --from=mcr.microsoft.com/dotnet/sdk:8.0-bookworm-slim@sha256:6ba9644da07bb5b560a402acec9ca38711a0762969f1b61da261df5c9f6f85da /usr/share/dotnet /usr/share/dotnet

# Debian trixie packages: C/C++ toolchain and bear, JDK 21 + Maven, Ruby, PHP, Python, .NET runtime libs.
# trixie (glibc 2.41) because scip-clang and scip-ruby need glibc 2.38.
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      bear ca-certificates cmake curl g++ gcc git libc6-dev libicu76 libssl3t64 make maven \
      openjdk-21-jdk-headless php-cli php-curl php-mbstring php-xml php-zip python3 python3-pip python3-venv \
      ruby-full unzip xz-utils \
 && rm -rf /var/lib/apt/lists/* \
 && ln -s /usr/share/dotnet/dotnet /usr/local/bin/dotnet

# Dart SDK 3.13.7 (scip_dart 1.8.1 needs >=3.12).
ADD --checksum=sha256:eb99aed266efbcb615c6ae21c12f52cfe935b9f208bccc5fea7f64c21e867229 \
    https://storage.googleapis.com/dart-archive/channels/stable/release/3.13.7/sdk/dartsdk-linux-x64-release.zip /tmp/dart.zip
RUN unzip -q /tmp/dart.zip -d /opt && rm /tmp/dart.zip

# scip-clang v0.4.1 (C and C++).
ADD --checksum=sha256:a434d26a079fd80e1996ba79276cb1ef86b2b2ad28b13e24386285db32c59046 \
    https://github.com/sourcegraph/scip-clang/releases/download/v0.4.1/scip-clang-x86_64-linux /usr/local/bin/scip-clang
RUN chmod 0755 /usr/local/bin/scip-clang

# scip-ruby 0.5.1 (Ruby): the x86_64-linux gem carries the native binary.
ADD --checksum=sha256:49319d7099ef62390379eb33ebadaee8f4e8706b50e2c88470ebad807cb6e9cf \
    https://github.com/sourcegraph/scip-ruby/releases/download/scip-ruby-v0.5.1/scip-ruby-0.5.1-x86_64-linux.gem /tmp/scip-ruby.gem
RUN gem install --local --no-document /tmp/scip-ruby.gem && rm /tmp/scip-ruby.gem \
 && ln -sf "$(gem contents scip-ruby | grep '/native/scip-ruby$')" /usr/local/bin/scip-ruby

# Composer 2.11.1 (PHP).
ADD --checksum=sha256:6f1b891e1d4927d62740276c540fb490e47689e19e102c49e8da849794882f8c \
    https://getcomposer.org/download/2.11.1/composer.phar /usr/local/lib/composer.phar
RUN printf '#!/bin/sh\nexec php /usr/local/lib/composer.phar "$@"\n' > /usr/local/bin/composer \
 && chmod 0755 /usr/local/bin/composer && chmod 0644 /usr/local/lib/composer.phar

# Coursier 2.1.26 bootstraps scip-java 0.12.3 (Java and Kotlin) into one standalone jar. 0.13.x writes SCIP
# `typed_range` only, which the `scip` crate the resolver reads (0.10.0, the latest) ignores; 0.14.0 needs a
# scip-kotlin-analysis artifact that is not on Maven Central.
ADD --checksum=sha256:348e37bc2a8c706640e6b032c4551a0e055a7f1537485b4a91540e9d3598ec6d \
    https://github.com/coursier/coursier/releases/download/v2.1.26/cs-x86_64-pc-linux.gz /tmp/cs.gz
RUN gunzip /tmp/cs.gz && mv /tmp/cs /usr/local/bin/cs && chmod 0755 /usr/local/bin/cs \
 && COURSIER_CACHE=/tmp/coursier cs bootstrap --standalone -o /usr/local/bin/scip-java \
      com.sourcegraph:scip-java_2.13:0.12.3 --main com.sourcegraph.scip_java.ScipJava \
 && rm -rf /tmp/coursier /tmp/cs.gz

# Rust 1.98.1 minimal profile plus rust-analyzer (the Rust indexer; the toolchain matches rust-toolchain.toml).
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo
ADD --checksum=sha256:b1c8cb14e30889e7e2999e6b69e62d9e9b9ffc0f4df8180564d306d9680e6c55 \
    https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init /tmp/rustup-init
RUN chmod 0755 /tmp/rustup-init \
 && /tmp/rustup-init -y --profile minimal --default-toolchain 1.98.1 -c rust-analyzer -c rust-src --no-modify-path \
 && rm /tmp/rustup-init \
 && rm -rf /opt/cargo/registry /opt/cargo/git

# Node indexers: scip-typescript 0.6.1 (TypeScript, JavaScript) and scip-python 0.6.8 (Python).
RUN npm install -g @sourcegraph/scip-typescript@0.6.1 @sourcegraph/scip-python@0.6.8 \
 && npm cache clean --force

# scip-dart 1.8.1 (Dart) and scip-dotnet 0.2.16 (C#), installed as global tools.
ENV PUB_CACHE=/opt/pub-cache \
    DOTNET_CLI_TELEMETRY_OPTOUT=1 \
    DOTNET_NOLOGO=1 \
    DOTNET_SKIP_FIRST_TIME_EXPERIENCE=1 \
    DOTNET_ROOT=/usr/share/dotnet
RUN PATH=/opt/dart-sdk/bin:$PATH dart pub global activate scip_dart 1.8.1 \
 && dotnet tool install --tool-path /opt/dotnet-tools --version 0.2.16 scip-dotnet \
 && rm -rf /root/.nuget /root/.cache /root/.dart /tmp/* \
 && chmod -R a+rwX /opt/pub-cache /opt/rustup /opt/cargo

ENV PATH=/opt/dart-sdk/bin:/opt/pub-cache/bin:/opt/dotnet-tools:/opt/cargo/bin:$PATH

WORKDIR /src
