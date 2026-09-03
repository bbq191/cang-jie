#!/bin/sh
# 交叉编译书架全部服务的 aarch64 **全静态** 二进制（reMarkable Paper Pro Move）。
# 前置同 reading/device-rs/build.sh：rustup target add aarch64-unknown-linux-musl + aarch64 交叉 gcc。
# 链接器与 CC/AR 在 .cargo/config.toml。
set -e
cd "$(dirname "$0")"

TARGET=aarch64-unknown-linux-musl
BINS="shelf-gateway book-serve koreader-serve font-serve wallpaper-serve"

echo "== host 构建 + 测试 =="
cargo build --release --workspace
cargo test --workspace --quiet

echo "== 交叉编译 $TARGET（全静态）=="
cargo build --release --workspace --target "$TARGET"

echo
echo "aarch64 全静态产物："
for b in $BINS; do
    f="target/$TARGET/release/$b"
    [ -f "$f" ] && echo "  $f  $(wc -c <"$f")B  $(file "$f" | grep -o 'statically linked' || echo dynamic)"
done
