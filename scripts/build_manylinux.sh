#!/bin/sh
# Host entry: build the already-verified source archive in a manylinux container.
if [ "$#" -lt 3 ]; then
    printf 'Usage: build_manylinux.sh CONTAINER TARGET PYTHON_ABI [PYTHON_ABI ...]\n' >&2
    exit 2
fi
container=$1
target=$2
shift 2 || exit 1
if [ ! -f source-build/pyproject.toml ] || [ ! -f source-build/Cargo.lock ]; then
    printf 'Extract the verified source distribution into source-build first\n' >&2
    exit 1
fi
image="turbo-manylinux:$target"
if docker build --build-arg "MANYLINUX_IMAGE=$container" --file scripts/manylinux.Dockerfile --tag "$image" scripts; then
    :
else
    printf 'Cannot provision the native uv manylinux build image\n' >&2
    exit 1
fi
if mkdir -p dist target; then
    :
else
    printf 'Cannot create wheel output and target directories\n' >&2
    exit 1
fi
if docker run --rm \
    --volume "$PWD/source-build:/build" \
    --volume "$PWD/dist:/dist" \
    --volume "$PWD/target:/target" \
    --volume "$PWD/scripts/build_manylinux_inner.sh:/build-wheel.sh:ro" \
    --workdir /build \
    --env CARGO_BUILD_WARNINGS=deny \
    --env CARGO_TARGET_DIR=/target \
    "$image" sh /build-wheel.sh "$target" "$@"; then
    :
else
    printf 'uv manylinux wheel build failed\n' >&2
    exit 1
fi
