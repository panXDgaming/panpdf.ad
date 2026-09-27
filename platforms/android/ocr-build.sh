#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
sdk="${ANDROID_HOME:-$HOME/Android/Sdk}"
ndk="$sdk/ndk/27.3.13750724"
src="$HOME/Android/src"
api=26
jobs="${CARGO_BUILD_JOBS:-6}"

LEPTONICA=1.85.0
LEPTONICA_SHA=3745ae3bf271a6801a2292eead83ac926e3a9bc1bf622e9cd4dd0f3786e17205
TESSERACT=5.5.1
TESSERACT_SHA=a7a3f2a7420cb6a6a94d80c24163e183cf1d2f1bed2df3bbc397c81808a57237

mkdir -p "$src"
fetch() {
    local file=$1 url=$2 sha=$3
    [[ -f $src/$file ]] || curl -sSL -o "$src/$file" "$url"
    echo "$sha  $src/$file" | sha256sum -c --quiet
}
fetch leptonica-$LEPTONICA.tar.gz \
    https://github.com/DanBloomberg/leptonica/releases/download/$LEPTONICA/leptonica-$LEPTONICA.tar.gz $LEPTONICA_SHA
fetch tesseract-$TESSERACT.tar.gz \
    https://github.com/tesseract-ocr/tesseract/archive/refs/tags/$TESSERACT.tar.gz $TESSERACT_SHA
[[ -d $src/leptonica-$LEPTONICA ]] || tar xzf "$src/leptonica-$LEPTONICA.tar.gz" -C "$src"
[[ -d $src/tesseract-$TESSERACT ]] || tar xzf "$src/tesseract-$TESSERACT.tar.gz" -C "$src"

which=${1-all}
case $which in
    all) abis=(arm64-v8a x86_64) ;;
    arm64) abis=(arm64-v8a) ;;
    x86_64) abis=(x86_64) ;;
    *) echo "usage: $0 [all|arm64|x86_64]" >&2; exit 2 ;;
esac

for abi in "${abis[@]}"; do
    work="$src/build-$abi"
    prefix="$work/prefix"
    common=(
        -DCMAKE_TOOLCHAIN_FILE="$ndk/build/cmake/android.toolchain.cmake"
        -DANDROID_ABI="$abi" -DANDROID_PLATFORM=android-$api -DANDROID_STL=c++_static
        -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$prefix"
        -DBUILD_SHARED_LIBS=OFF -DCMAKE_FIND_ROOT_PATH="$prefix"
        -DANDROID_SUPPORT_FLEXIBLE_PAGE_SIZES=ON
        -DCMAKE_EXE_LINKER_FLAGS=-Wl,-z,max-page-size=16384
        -DCMAKE_SHARED_LINKER_FLAGS=-Wl,-z,max-page-size=16384
        -DCMAKE_C_FLAGS=-ffile-prefix-map=$HOME=~
        -DCMAKE_CXX_FLAGS=-ffile-prefix-map=$HOME=~
    )
    echo "=== leptonica ($abi)"
    cmake -S "$src/leptonica-$LEPTONICA" -B "$work/leptonica" "${common[@]}" \
        -DENABLE_ZLIB=OFF -DENABLE_PNG=OFF -DENABLE_GIF=OFF -DENABLE_JPEG=OFF \
        -DENABLE_TIFF=OFF -DENABLE_WEBP=OFF -DENABLE_OPENJPEG=OFF -DBUILD_PROG=OFF \
        -DSW_BUILD=OFF >/dev/null
    cmake --build "$work/leptonica" -j "$jobs" >/dev/null
    cmake --install "$work/leptonica" >/dev/null
    compat="$prefix/lib/cmake/CpuFeaturesNdkCompat"
    mkdir -p "$compat" "$prefix/include/ndk_compat" "$prefix/lib"
    case $abi in
        arm64-v8a) triple=aarch64-linux-android ;;
        x86_64) triple=x86_64-linux-android ;;
    esac
    llvm="$ndk/toolchains/llvm/prebuilt/linux-x86_64/bin"
    cp "$ndk/sources/android/cpufeatures/cpu-features.h" "$prefix/include/ndk_compat/"
    "$llvm/${triple}${api}-clang" -O2 -c "$ndk/sources/android/cpufeatures/cpu-features.c" \
        -o "$work/cpu-features.o"
    "$llvm/llvm-ar" rcs "$prefix/lib/libcpufeatures.a" "$work/cpu-features.o"
    cat > "$compat/CpuFeaturesNdkCompatConfig.cmake" <<CMAKE
add_library(CpuFeatures::ndk_compat STATIC IMPORTED)
set_target_properties(CpuFeatures::ndk_compat PROPERTIES
    IMPORTED_LOCATION "$prefix/lib/libcpufeatures.a"
    INTERFACE_INCLUDE_DIRECTORIES "$prefix/include/ndk_compat")
CMAKE
    echo "=== tesseract ($abi)"
    cmake -S "$src/tesseract-$TESSERACT" -B "$work/tesseract" "${common[@]}" \
        -DLeptonica_DIR="$prefix/lib/cmake/leptonica" -DCpuFeaturesNdkCompat_DIR="$compat" \
        -DBUILD_TRAINING_TOOLS=OFF -DGRAPHICS_DISABLED=ON -DDISABLE_ARCHIVE=ON \
        -DDISABLE_CURL=ON -DOPENMP_BUILD=OFF -DBUILD_TESTS=OFF -DSW_BUILD=OFF \
        -DENABLE_LTO=OFF -DDISABLE_TIFF=ON \
        -DLEPT_TIFF_RESULT=1 >/dev/null  # a cross build cannot try-run: Leptonica has no TIFF here
    cmake --build "$work/tesseract" -j "$jobs" --target tesseract >/dev/null
    mkdir -p "$here/ocr/$abi"
    program=$(find "$work/tesseract" -maxdepth 2 -type f -name tesseract -perm -u+x | head -1)
    "$ndk/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-strip" -o "$here/ocr/$abi/libtesseract.so" "$program"
    ls -lh "$here/ocr/$abi/libtesseract.so"
done
