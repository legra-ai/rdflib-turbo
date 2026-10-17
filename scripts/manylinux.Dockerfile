ARG MANYLINUX_IMAGE=quay.io/pypa/manylinux_2_28_x86_64:latest
FROM ghcr.io/astral-sh/uv:0.13.1 AS uv
FROM ${MANYLINUX_IMAGE}
COPY --from=uv /uv /usr/local/bin/uv
# bindgen needs the Clang shared library while building bundled RocksDB.
RUN dnf install -y clang clang-libs || exit 1
RUN dnf clean all || exit 1
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/turbo-rustup-init.sh || exit 1
RUN sh /tmp/turbo-rustup-init.sh -y --profile minimal --default-toolchain 1.99.1 || exit 1
ENV PATH="/root/.cargo/bin:${PATH}"
