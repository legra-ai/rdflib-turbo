#!/bin/sh
# Container entry: provision the pinned compiler and build through PEP 517 with uv.
if [ "$#" -ne 2 ]; then
    printf 'Expected target triple and Python ABI\n' >&2
    exit 2
fi
if uv build --wheel --python "/opt/python/$2/bin/python" --out-dir /dist \
    --config-setting "maturin.build-args=--locked --target $1 --compatibility manylinux_2_28"; then
    :
else
    printf 'uv failed to build the verified source archive\n' >&2
    exit 1
fi
