#!/bin/bash

# Start storage proxy.
# (`-B ""` to skip build command.)
miniflare --modules --modules-rule=CompiledWasm=**/*.wasm /build/worker/shim.mjs -B "" &

# Start service.
/service -c /configuration-helper.toml &

wait -n
exit $?
