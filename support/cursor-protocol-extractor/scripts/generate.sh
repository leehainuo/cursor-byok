#!/usr/bin/env bash
# generate.sh 根据提取的 Proto 定义生成可供其他 Go module 使用的消息包。
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPOSITORY_DIR="$(cd "$PROJECT_DIR/../.." && pwd)"
PROTO_DIR="$REPOSITORY_DIR/protocols/cursor"
MODULE_PATH="github.com/leookun/cursor-byok/cursor-proto"

command -v protoc >/dev/null 2>&1 || {
  echo "protoc is required" >&2
  exit 1
}
command -v protoc-gen-go >/dev/null 2>&1 || {
  echo "protoc-gen-go is required" >&2
  exit 1
}

for PROTO_FILE in agent_v1.proto aiserver_v1.proto; do
  if [[ ! -f "$PROTO_DIR/$PROTO_FILE" ]]; then
    echo "Missing Proto source: $PROTO_DIR/$PROTO_FILE" >&2
    exit 1
  fi
done

protoc \
  --proto_path="$PROTO_DIR" \
  --go_out="$PROJECT_DIR" \
  --go_opt="module=$MODULE_PATH" \
  "$PROTO_DIR/agent_v1.proto" \
  "$PROTO_DIR/aiserver_v1.proto"

echo "Generated Go packages under: $PROJECT_DIR/gen"
