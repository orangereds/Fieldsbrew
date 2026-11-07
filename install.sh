#!/bin/sh
set -eu

PREFIX="${FIELDSBREW_PREFIX:-/opt/fieldsbrew}"

usage() {
  cat <<'EOF'
用法: ./install.sh [--prefix DIR]

把当前压缩包内的 fb 安装到 DIR/bin（默认 /opt/fieldsbrew）。
也可通过环境变量 FIELDSBREW_PREFIX 指定前缀。
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --prefix)
      PREFIX="$2"
      shift 2
      ;;
    --prefix=*)
      PREFIX="${1#*=}"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "未知参数: $1" >&2
      usage
      exit 1
      ;;
  esac
done

case "$(uname -m)" in
  x86_64) ;;
  *)
    echo "Fieldsbrew 当前仅支持 Intel Mac (x86_64)，检测到 $(uname -m)" >&2
    exit 1
    ;;
esac

DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
BIN="$DIR/fb"
if [ ! -f "$BIN" ]; then
  BIN="$DIR/bin/fb"
fi
if [ ! -f "$BIN" ]; then
  echo "压缩包中未找到 fb 二进制" >&2
  exit 1
fi

if ! mkdir -p "$PREFIX/bin" 2>/dev/null; then
  echo "无法写入 $PREFIX" >&2
  echo "请用 sudo 运行，或设置 FIELDSBREW_PREFIX 指向可写目录" >&2
  exit 1
fi

cp "$BIN" "$PREFIX/bin/fb"
chmod 755 "$PREFIX/bin/fb"

echo "已安装: $PREFIX/bin/fb"
"$PREFIX/bin/fb" --version
echo
echo "把下面一行加入 ~/.zshrc 或 ~/.bashrc:"
echo "  eval \"\$($PREFIX/bin/fb shellenv)\""
echo
echo "然后执行: fb doctor"
