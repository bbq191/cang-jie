#!/bin/sh
# 交叉编译书架全部服务的 aarch64 **全静态** 二进制（reMarkable Paper Pro Move）。
# 前置：rustup target add aarch64-unknown-linux-musl + aarch64 交叉 gcc。
# 链接器与 CC/AR 在 .cargo/config.toml。
set -e
cd "$(dirname "$0")"
# shellcheck disable=SC1091
. ./manifest.sh   # SHELF_ALL / shelf_svc_of / shelf_svc_home：要编哪些项目、列哪些产物，与 packaging/deploy.sh 同一份清单

TARGET=aarch64-unknown-linux-musl
# 要部署的服务与它们所在的顶层 Cargo 项目都取自 manifest.sh（2026-10-10 前这里另写一份 BINS/GATEWAY_BINS/ENHANCE_BINS/NOTES_BINS，
# 审计 PK-2）。网关（../gateway）、笔记线（../notes）、enhance 的 wallpaper-serve/font-serve 都是独立顶层 Cargo 项目，随书架一起
# 编/装（目录不存在则跳过）；网关是 shelf/notes/enhance 三条线共用的唯一前端，见 ../gateway/README.md。
# cargo 一律 --locked（同 CI）：Cargo.lock 与 Cargo.toml 对不上就失败，不在打包时悄悄重新解析依赖。

# 各服务所在项目，去重、保持 SHELF_ALL 的顺序
HOMES=""
for s in $SHELF_ALL; do
    h="$(shelf_svc_home "$s")"
    case " $HOMES " in *" $h "*) ;; *) HOMES="$HOMES $h" ;; esac
done

echo "== host 构建 + 测试 =="
cargo build --release --workspace --locked
cargo test --workspace --locked --quiet

echo "== 交叉编译 $TARGET（全静态）=="
cargo build --release --workspace --locked --target "$TARGET"
for h in $HOMES; do
    [ "$h" = shelf ] && continue   # 书架自己上面已经编过
    if [ -f "../$h/Cargo.toml" ]; then
        echo "== $h/：host 测试 + 交叉编译 =="
        (cd "../$h" && cargo test --workspace --locked --quiet && cargo build --release --workspace --locked --target "$TARGET")
    fi
done

echo
echo "aarch64 全静态产物："
# 只列出存在的产物（if 而不是 `[ -f ] && echo`：后者在最后一项缺失时会让整个脚本以 1 退出）
report() {
    if [ -f "$1" ]; then echo "  $1  $(wc -c <"$1")B  $(file "$1" | grep -o 'statically linked' || echo dynamic)"; fi
}
for s in $SHELF_ALL; do report "../$(shelf_svc_home "$s")/target/$TARGET/release/$(shelf_svc_of "$s")"; done
