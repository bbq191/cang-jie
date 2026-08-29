#!/bin/sh
# 交叉编译 pkm（块5 PKM）的 aarch64 全静态二进制（reMarkable Paper Pro Move）。
# 前置同 ../../reading/device-rs/build.sh：rustup target add aarch64-unknown-linux-musl + aarch64-gcc。
# 依赖块3阅读 crate（../../reading/device-rs，path 依赖，cargo 自动一并编）。
set -e
cd "$(dirname "$0")"

TARGET=aarch64-unknown-linux-musl

echo "== host 构建（自测/对拍用）=="
cargo build --release

echo "== 交叉编译 $TARGET（全静态）=="
cargo build --release --target "$TARGET"

echo
echo "aarch64 全静态产物："
for b in wr-stars wr-stars-daemon wr-nbtest wr-cardhw; do
    f="target/$TARGET/release/$b"
    [ -f "$f" ] && echo "  $f  $(wc -c <"$f")B  $(file "$f" | grep -o 'statically linked' || echo dynamic)"
done
