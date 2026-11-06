# Swift language server for scripts/corpus/lsp2scip.py (the LSP bridge). Built by `make corpus-image-swift`.
# Separate from Dockerfile.lsp: the Swift toolchain is large and only this entry needs it.
#
# linux/amd64 only. The base is the official Swift image, pinned by its multi-platform index digest; sourcekit-lsp
# ships in its toolchain (/usr/bin). Bump the digest with the Swift version and re-run `make corpus-index FORCE=1`.

FROM swift:6.1-noble@sha256:92cafdba8c519c32b83d5aa50539fb3d5a534bc87c3d9437bf6e8984aa1be4f3

# python3 for the bridge.
RUN apt-get update \
 && apt-get install -y --no-install-recommends python3 \
 && rm -rf /var/lib/apt/lists/*

# The bridge itself (stdlib only): scripts/corpus/lsp2scip.py.
COPY lsp2scip.py /usr/local/lib/ripwire/lsp2scip.py

WORKDIR /src
