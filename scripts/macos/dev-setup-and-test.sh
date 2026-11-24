#!/bin/sh
# Prepare a Mac that has Xcode Command Line Tools and no Homebrew, then build
# and test HUSH. Run as a normal user. Installing the HAL plugin is a separate
# sudo step: scripts/macos/install-driver.sh.
set -eu

if [ "$(uname -s)" != "Darwin" ]; then
    echo "This script is for macOS. It was not run." >&2
    exit 1
fi

root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
prefix="${HOME}/.local/hush-dev"
mkdir -p "$prefix/bin"
export PATH="${HOME}/.cargo/bin:${prefix}/bin:${PATH}"

if ! xcode-select -p >/dev/null 2>&1; then
    echo "Install Xcode Command Line Tools first: xcode-select --install" >&2
    exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
    echo "python3 is missing. The Command Line Tools package provides it." >&2
    exit 1
fi

if ! command -v rustc >/dev/null 2>&1; then
    echo "Installing rustup into ${HOME}/.cargo"
    curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
fi
if [ -f "${HOME}/.cargo/env" ]; then
    # shellcheck disable=SC1091
    . "${HOME}/.cargo/env"
fi
export PATH="${HOME}/.cargo/bin:${prefix}/bin:${PATH}"

if ! command -v cmake >/dev/null 2>&1; then
    echo "Installing CMake into ${prefix}"
    cmake_ver=3.31.6
    tmp=$(mktemp -d)
    curl -fL --retry 3 -o "$tmp/cmake.tar.gz" \
        "https://github.com/Kitware/CMake/releases/download/v${cmake_ver}/cmake-${cmake_ver}-macos-universal.tar.gz"
    tar -xzf "$tmp/cmake.tar.gz" -C "$tmp"
    mkdir -p "$prefix/cmake"
    cp -R "$tmp/cmake-${cmake_ver}-macos-universal/CMake.app" "$prefix/cmake/"
    ln -sfn "$prefix/cmake/CMake.app/Contents/bin/cmake" "$prefix/bin/cmake"
    ln -sfn "$prefix/cmake/CMake.app/Contents/bin/ctest" "$prefix/bin/ctest"
    ln -sfn "$prefix/cmake/CMake.app/Contents/bin/cpack" "$prefix/bin/cpack"
    rm -rf "$tmp"
fi

if ! command -v ninja >/dev/null 2>&1; then
    echo "Installing Ninja into ${prefix}"
    tmp=$(mktemp -d)
    curl -fL --retry 3 -o "$tmp/ninja.zip" \
        "https://github.com/ninja-build/ninja/releases/download/v1.12.1/ninja-mac.zip"
    unzip -o "$tmp/ninja.zip" -d "$prefix/bin"
    chmod 755 "$prefix/bin/ninja"
    rm -rf "$tmp"
fi

if ! command -v meson >/dev/null 2>&1; then
    echo "Installing Meson into ${prefix}"
    tmp=$(mktemp -d)
    curl -fL --retry 3 -o "$tmp/meson.tar.gz" \
        "https://github.com/mesonbuild/meson/releases/download/1.6.1/meson-1.6.1.tar.gz"
    tar -xzf "$tmp/meson.tar.gz" -C "$prefix"
    rm -rf "$prefix/meson"
    mv "$prefix/meson-1.6.1" "$prefix/meson"
    cat > "$prefix/bin/meson" <<EOF
#!/bin/sh
exec python3 "$prefix/meson/meson.py" "\$@"
EOF
    chmod 755 "$prefix/bin/meson"
    rm -rf "$tmp"
fi

model_dir="${HOME}/.cache/dpdfnet/models"
model="${model_dir}/dpdfnet8_48khz_hr.onnx"
if [ ! -s "$model" ]; then
    echo "Downloading DPDFNet8 into ${model_dir}"
    mkdir -p "$model_dir"
    curl -fL --retry 3 -o "$model" \
        "https://huggingface.co/Ceva-IP/DPDFNet/resolve/main/onnx/dpdfnet8_48khz_hr.onnx?download=true"
fi

export CC="${CC:-clang}"
export CXX="${CXX:-clang++}"
cd "$root"
jobs=$(sysctl -n hw.ncpu)
export CARGO_BUILD_JOBS="$jobs"

echo "Building and testing the Rust core, FFI, and CLI"
cargo test -p hush-core -p hush-ffi -p hush-cli --release -- --test-threads="$jobs"
# cargo test builds the test harness, not target/release/hush.
echo "Building the hush CLI"
cargo build -p hush-cli --release

echo "Building the arm64 HAL plugin"
(
    cd "$root/drivers/macos"
    ./build.sh
)

echo "CLI is at ${root}/target/release/hush"
echo "Driver bundle is at ${root}/drivers/macos/build/HUSH.driver"
echo "Install it in a separate step, then run the device check:"
echo "  sudo ${root}/scripts/macos/install-driver.sh"
echo "  ${root}/target/release/hush selftest"
