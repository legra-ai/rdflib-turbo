#!/bin/sh
# Build one stable-ABI wheel from the verified source archive through uv.
if [ "$#" -ne 1 ]; then
    printf 'Expected exactly one target triple\n' >&2
    exit 2
fi
target=$1
if uv build --wheel --python /opt/python/cp311-cp311/bin/python --out-dir /dist \
    --config-setting "maturin.build-args=--locked --target $target --compatibility manylinux_2_28"; then
    :
else
    printf 'uv failed to build the verified stable-ABI source archive\n' >&2
    exit 1
fi
