#!/bin/sh
# Container entry: provision the pinned compiler and build through PEP 517 with uv.
if [ "$#" -lt 2 ]; then
    printf 'Expected target triple and at least one Python ABI\n' >&2
    exit 2
fi
target=$1
shift || exit 1
# One container and source path retain the vendor/compiler cache across interpreters.
for python_abi in "$@"; do
    if uv build --wheel --python "/opt/python/$python_abi/bin/python" --out-dir /dist \
        --config-setting "maturin.build-args=--locked --target $target --compatibility manylinux_2_28"; then
        :
    else
        printf 'uv failed to build the verified source archive for %s\n' "$python_abi" >&2
        exit 1
    fi
done
