#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
for name in pdf_tool panpdf-convert; do
    if [[ -d "$here/../../../$name" ]]; then
        convert="$(cd "$here/../../../$name" && pwd)"
        break
    fi
done
if [[ -z ${convert-} ]]; then
    echo "the converters are not beside this checkout: clone pdf_tool next to it" >&2
    exit 1
fi
sdk="${ANDROID_HOME:-$HOME/Android/Sdk}"
llvm="$sdk/ndk/27.3.13750724/toolchains/llvm/prebuilt/linux-x86_64/bin"
api=26

which=${1-all}
case $which in
    all) targets=(aarch64-linux-android x86_64-linux-android) ;;
    arm64) targets=(aarch64-linux-android) ;;
    x86_64) targets=(x86_64-linux-android) ;;
    *) echo "usage: $0 [all|arm64|x86_64]" >&2; exit 2 ;;
esac
for target in "${targets[@]}"; do
    upper=$(echo "$target" | tr 'a-z-' 'A-Z_')
    under=$(echo "$target" | tr '-' '_')
    export "CARGO_TARGET_${upper}_LINKER=$llvm/${target}${api}-clang"
    export "CARGO_TARGET_${upper}_RUSTFLAGS=-C link-arg=-Wl,-z,max-page-size=16384 --remap-path-prefix=$HOME=~"
    export "CC_${under}=$llvm/${target}${api}-clang"
    export "AR_${under}=$llvm/llvm-ar"
    echo "=== panpdf-tools ($target)"
    (cd "$convert" && cargo build --release --target "$target" -p panpdf-tools)
    case $target in
        aarch64-linux-android) abi=arm64-v8a ;;
        x86_64-linux-android) abi=x86_64 ;;
    esac
    mkdir -p "$here/tools/$abi"
    "$llvm/llvm-strip" -o "$here/tools/$abi/libpanpdftools.so" "$convert/target/$target/release/panpdf-tools"
    ls -lh "$here/tools/$abi/libpanpdftools.so"
done
