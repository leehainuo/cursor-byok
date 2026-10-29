#!/usr/bin/env bash
# extract.sh 从 Cursor 安装目录安全提取 Proto 文件。
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPOSITORY_DIR="$(cd "$PROJECT_DIR/../.." && pwd)"
INSTALLED_CURSOR_DEFAULT="/Applications/Cursor.app"
INPUT_DEFAULT="$INSTALLED_CURSOR_DEFAULT"
OUTPUT_DEFAULT="$REPOSITORY_DIR/protocols/cursor"

INPUT_ROOT="${1:-$INPUT_DEFAULT}"
OUTPUT_DIR="${2:-$OUTPUT_DEFAULT}"

canonicalize_path() {
  local path="$1"
  local parent
  local base
  if [[ -d "$path" ]]; then
    (cd "$path" && pwd -P)
    return
  fi
  parent="$(dirname "$path")"
  base="$(basename "$path")"
  if [[ ! -d "$parent" ]]; then
    echo "Parent directory does not exist: $parent" >&2
    return 1
  fi
  printf '%s/%s\n' "$(cd "$parent" && pwd -P)" "$base"
}

# 文件输入只提取自身；目录输入扫描工作台、扩展宿主和扩展产物。
INPUT_PATHS=()

add_input() {
  local candidate="$1"
  local existing
  if [[ ! -f "$candidate" ]]; then
    return 0
  fi
  for existing in "${INPUT_PATHS[@]-}"; do
    [[ "$existing" == "$candidate" ]] && return
  done
  INPUT_PATHS+=("$candidate")
}

if [[ -f "$INPUT_ROOT" ]]; then
  add_input "$INPUT_ROOT"
elif [[ -d "$INPUT_ROOT" ]]; then
  CANDIDATES=(
    "$INPUT_ROOT/Contents/Resources/app/out/vs/workbench/workbench.desktop.main.js"
    "$INPUT_ROOT/Resources/app/out/vs/workbench/workbench.desktop.main.js"
    "$INPUT_ROOT/out/vs/workbench/workbench.desktop.main.js"
    "$INPUT_ROOT/workbench.desktop.main.js"
    "$INPUT_ROOT/Contents/Resources/app/out/vs/workbench/api/node/extensionHostProcess.js"
    "$INPUT_ROOT/Resources/app/out/vs/workbench/api/node/extensionHostProcess.js"
    "$INPUT_ROOT/out/vs/workbench/api/node/extensionHostProcess.js"
    "$INPUT_ROOT/extensionHostProcess.js"
    "$INPUT_ROOT/Contents/Resources/app/extensions/cursor-always-local/dist/main.js"
    "$INPUT_ROOT/Resources/app/extensions/cursor-always-local/dist/main.js"
    "$INPUT_ROOT/extensions/cursor-always-local/dist/main.js"
    "$INPUT_ROOT/cursor-always-local/dist/main.js"
  )
  for CANDIDATE in "${CANDIDATES[@]}"; do
    add_input "$CANDIDATE"
  done
  while IFS= read -r JS_FILE; do
    add_input "$JS_FILE"
  done < <(find "$INPUT_ROOT" -type f ! -path "*/node_modules/*" \( -name "workbench.desktop.main.js" -o -name "extensionHostProcess.js" -o -path "*/extensions/*/dist/main.js" \) | sort)
fi

if [[ -z "${INPUT_PATHS[*]-}" ]]; then
  echo "No supported Cursor JS bundle found under: $INPUT_ROOT" >&2
  echo "Install/update Cursor, or pass an explicit input bundle:" >&2
  echo "  $0 /path/to/Cursor.app [output-dir]" >&2
  exit 1
fi

for INDEX in "${!INPUT_PATHS[@]}"; do
  INPUT_PATHS[$INDEX]="$(canonicalize_path "${INPUT_PATHS[$INDEX]}")"
done
OUTPUT_DIR="$(canonicalize_path "$OUTPUT_DIR")"
CURRENT_DIR="$(pwd -P)"

case "$OUTPUT_DIR" in
  "/"|"$HOME"|"$PROJECT_DIR"|"$SCRIPT_DIR"|"$CURRENT_DIR")
    echo "Refusing unsafe output directory: $OUTPUT_DIR" >&2
    exit 1
    ;;
esac
for INPUT_PATH in "${INPUT_PATHS[@]}"; do
  case "$INPUT_PATH" in
    "$OUTPUT_DIR"|"$OUTPUT_DIR"/*)
      echo "Refusing output directory that contains an input bundle: $OUTPUT_DIR" >&2
      exit 1
      ;;
  esac
done

OUTPUT_PARENT="$(dirname "$OUTPUT_DIR")"
OUTPUT_BASENAME="$(basename "$OUTPUT_DIR")"
TEMP_DIR="$(mktemp -d "$OUTPUT_PARENT/.${OUTPUT_BASENAME}.tmp.XXXXXX")"
BACKUP_DIR=""

cleanup() {
  if [[ -n "$TEMP_DIR" && -d "$TEMP_DIR" ]]; then
    rm -rf "$TEMP_DIR"
  fi
  if [[ -n "$BACKUP_DIR" && -e "$BACKUP_DIR" ]]; then
    if [[ ! -e "$OUTPUT_DIR" ]]; then
      mv "$BACKUP_DIR" "$OUTPUT_DIR"
    else
      rm -rf "$BACKUP_DIR"
    fi
  fi
}
trap cleanup EXIT

EXTRACT_ARGS=()
for INPUT_PATH in "${INPUT_PATHS[@]}"; do
  EXTRACT_ARGS+=( -input "$INPUT_PATH" )
done

(
  cd "$PROJECT_DIR"
  go run ./extractor \
    "${EXTRACT_ARGS[@]}" \
    -output "$TEMP_DIR" \
    -skip-format \
    -strict
)

if [[ -e "$OUTPUT_DIR" ]]; then
  BACKUP_DIR="$(mktemp -d "$OUTPUT_PARENT/.${OUTPUT_BASENAME}.backup.XXXXXX")"
  rmdir "$BACKUP_DIR"
  mv "$OUTPUT_DIR" "$BACKUP_DIR"
fi
mv "$TEMP_DIR" "$OUTPUT_DIR"
TEMP_DIR=""
if [[ -n "$BACKUP_DIR" ]]; then
  rm -rf "$BACKUP_DIR"
  BACKUP_DIR=""
fi
