#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
sdk="${ANDROID_HOME:-$HOME/Android/Sdk}"
ndk="$sdk/ndk/27.3.13750724"
tools="$sdk/build-tools/35.0.0"
platform="$sdk/platforms/android-35/android.jar"
api=26
llvm="$ndk/toolchains/llvm/prebuilt/linux-x86_64/bin"
out="$here/out"
mkdir -p "$out"

which=${1-all}
targets=()
case $which in
    all) targets=(aarch64-linux-android x86_64-linux-android) ;;
    arm64) targets=(aarch64-linux-android) ;;
    x86_64) targets=(x86_64-linux-android) ;;
    *) echo "usage: $0 [all|arm64|x86_64]" >&2; exit 2 ;;
esac

for target in "${targets[@]}"; do
    [[ ${PACKAGE_ONLY-} ]] && break
    upper=$(echo "$target" | tr 'a-z-' 'A-Z_')
    under=$(echo "$target" | tr '-' '_')
    export "CARGO_TARGET_${upper}_LINKER=$llvm/${target}${api}-clang"
    export "CARGO_TARGET_${upper}_RUSTFLAGS=-C link-arg=-Wl,-z,max-page-size=16384 --remap-path-prefix=$HOME=~"
    export "CC_${under}=$llvm/${target}${api}-clang"
    export "CXX_${under}=$llvm/${target}${api}-clang++"
    export "AR_${under}=$llvm/llvm-ar"
    echo "=== cargo build ($target)"
    (cd "$root/engine" && cargo build --release --target "$target" -p pdf-android)
done

echo "=== package"
stage="$out/stage"
rm -rf "$stage"
mkdir -p "$stage"
for target in "${targets[@]}"; do
    case $target in
        aarch64-linux-android) abi=arm64-v8a ;;
        x86_64-linux-android) abi=x86_64 ;;
    esac
    mkdir -p "$stage/lib/$abi"
    cp "$root/engine/target/$target/release/libpanpdf.so" "$stage/lib/$abi/"
    "$llvm/llvm-strip" --strip-unneeded "$stage/lib/$abi/libpanpdf.so"
    if [[ -f $here/tools/$abi/libpanpdftools.so ]]; then
        cp "$here/tools/$abi/libpanpdftools.so" "$stage/lib/$abi/"
    else
        echo "no tools/$abi/libpanpdftools.so: run platforms/android/tools-build.sh" >&2
    fi
    if [[ -f $here/ocr/$abi/libtesseract.so ]]; then
        cp "$here/ocr/$abi/libtesseract.so" "$stage/lib/$abi/"
    else
        echo "no ocr/$abi/libtesseract.so: run platforms/android/ocr-build.sh" >&2
    fi
done

classes="$out/classes"
rm -rf "$classes" && mkdir -p "$classes"
javac --release 17 -nowarn -classpath "$platform" -d "$classes" \
    $(find "$here/java" -name '*.java')
"$tools/d8" --release --min-api "$api" --lib "$platform" --output "$stage" \
    $(find "$classes" -name '*.class')

assets="$out/assets"
rm -rf "$assets" && mkdir -p "$assets/fonts/packaged"
cp "$root/engine/fonts/manifest.json" "$assets/fonts/"
find "$root/engine/fonts/packaged" -maxdepth 1 -type f ! -name 'NotoSansCJK*' \
    -exec cp {} "$assets/fonts/packaged/" \;

compiled="$out/res.zip"
rm -f "$compiled"
"$tools/aapt2" compile --dir "$here/res" -o "$compiled"
"$tools/aapt2" link -o "$out/unsigned.apk" -I "$platform" -A "$assets" "$compiled" \
    --manifest "$here/AndroidManifest.xml" --min-sdk-version "$api" --target-sdk-version 35
(cd "$stage" && zip -q -0 -r "$out/unsigned.apk" lib && zip -q "$out/unsigned.apk" classes.dex)
"$tools/zipalign" -f -P 16 4 "$out/unsigned.apk" "$out/aligned.apk"

keystore="$out/debug.keystore"
if [[ ! -f $keystore ]]; then
    keytool -genkeypair -keystore "$keystore" -storepass android -keypass android \
        -alias debug -keyalg RSA -keysize 2048 -validity 10000 \
        -dname "CN=PanPDF debug" >/dev/null 2>&1
fi
"$tools/apksigner" sign --ks "$keystore" --ks-pass pass:android --key-pass pass:android \
    --out "$out/panpdf.apk" "$out/aligned.apk"
rm -f "$out/unsigned.apk" "$out/aligned.apk"
ls -lh "$out/panpdf.apk"
