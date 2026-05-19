#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"
BACKEND_ENV_FILE="$ROOT_DIR/.mossensrc/custom-backend.env"
FEATURE_ENV_FILE="$ROOT_DIR/.mossensrc/feature-flags.env"
LAUNCH_CWD="${MOSSENSRC_LAUNCH_CWD:-$PWD}"
SETTINGS_FILE="${HOME}/.mossen/settings.json"

load_env_file() {
  local file="$1"
  if [[ -f "$file" ]]; then
    # shellcheck disable=SC1090
    source "$file"
  fi
}

normalize_features() {
  local raw="$1"
  if [[ -z "${raw//[[:space:],]/}" ]]; then
    return
  fi

  local normalized
  normalized="$(printf '%s' "$raw" | tr ',' '\n')"
  while IFS= read -r feature_name; do
    feature_name="${feature_name#"${feature_name%%[![:space:]]*}"}"
    feature_name="${feature_name%"${feature_name##*[![:space:]]}"}"
    if [[ -n "$feature_name" ]]; then
      printf '%s\n' "$feature_name"
    fi
  done <<<"$normalized"
}

load_env_file "$BACKEND_ENV_FILE"
load_env_file "$FEATURE_ENV_FILE"

set_launch_locale_from_settings() {
  if [[ ! -f "$SETTINGS_FILE" ]]; then
    return
  fi

  local interactive_language
  interactive_language="$(python3 - "$SETTINGS_FILE" <<'PY'
import json, sys
from pathlib import Path

try:
    raw = json.loads(Path(sys.argv[1]).read_text(encoding='utf-8'))
except Exception:
    print('')
    raise SystemExit(0)

language = raw.get('language')
if not isinstance(language, str):
    print('')
    raise SystemExit(0)

value = language.strip().lower()
if (
    value == 'zn'
    or value == 'cn'
    or value.startswith('zh')
    or '中文' in value
    or '汉语' in value
    or '漢語' in value
    or '简体' in value
    or '繁体' in value
    or '繁體' in value
    or 'chinese' in value
    or 'mandarin' in value
):
    print('zh')
elif value:
    print('en')
else:
    print('')
PY
)"

  if [[ -z "$interactive_language" ]]; then
    return
  fi

  export MOSSENSRC_INTERACTIVE_LANGUAGE="$interactive_language"
  export MOSSEN_UI_LANGUAGE="$interactive_language"

  if [[ "$interactive_language" == "zh" ]]; then
    export LANG="zh_CN.UTF-8"
    export LC_MESSAGES="zh_CN.UTF-8"
  else
    export LANG="en_US.UTF-8"
    export LC_MESSAGES="en_US.UTF-8"
  fi
}

set_launch_locale_from_settings

declare -a bun_args
bun_args=(bun)

while IFS= read -r feature_name; do
  if [[ " ${bun_args[*]} " == *" --feature=$feature_name "* ]]; then
    continue
  fi
  bun_args+=("--feature=$feature_name")
done < <(normalize_features "${MOSSENSRC_BUN_FEATURES:-}")

declare -a exec_args
exec_args=("$@")

if [[ ${#exec_args[@]} -gt 0 ]]; then
  first_arg="${exec_args[0]}"
  if [[ "$first_arg" != /* && -e "$ROOT_DIR/$first_arg" ]]; then
    exec_args[0]="$ROOT_DIR/$first_arg"
  fi
fi

# Wave 7 Door Lock — CLI shell entrypoint sanitizer.
# Locks USER_TYPE before Bun loads any module so top-level conditional
# requires (e.g. tools.ts ant-only require) see the public value. Active only
# for the real CLI entrypoint (entrypoints/cli.tsx); other Bun invocations
# (`bun -e`, `--eval`, custom entries) pass through so test/dev paths can
# still exercise raw USER_TYPE values.
# Rules mirror utils/userTypeRuntimeLock.ts:
#   unset/empty/external/unknown                                 -> external
#   ant|mossen with MOSSEN_CODE_ALLOW_INTERNAL_USER_TYPE = "1"   -> raw
#   ant|mossen otherwise                                          -> external
if [[ ${#exec_args[@]} -gt 0 ]] && {
  [[ "${exec_args[0]}" == "entrypoints/cli.tsx" ]] ||
  [[ "${exec_args[0]}" == "$ROOT_DIR/entrypoints/cli.tsx" ]]
}; then
  _mossen_raw_user_type="${USER_TYPE:-}"
  _mossen_unlock_user_type="${MOSSEN_CODE_ALLOW_INTERNAL_USER_TYPE:-}"
  if [[ "$_mossen_unlock_user_type" == "1" ]] && {
    [[ "$_mossen_raw_user_type" == "ant" ]] ||
    [[ "$_mossen_raw_user_type" == "mossen" ]]
  }; then
    export USER_TYPE="$_mossen_raw_user_type"
  else
    export USER_TYPE="external"
  fi
  unset _mossen_raw_user_type _mossen_unlock_user_type
fi

export MOSSENSRC_LAUNCH_CWD="$LAUNCH_CWD"
cd "$ROOT_DIR"

exec "${bun_args[@]}" "${exec_args[@]}"
