#!/usr/bin/env bash
# Linux/WSL2 向け UI パネル Wasm ビルドスクリプト
# PowerShell が使えない環境では本スクリプトを使う
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TARGET="wasm32-unknown-unknown"
RELEASE=0

for arg in "$@"; do
  case "$arg" in
    --release) RELEASE=1 ;;
  esac
done

PROFILE_DIR="debug"
CARGO_ARGS=("build" "--target" "$TARGET")
if [ "$RELEASE" -eq 1 ]; then
  CARGO_ARGS+=("--release")
  PROFILE_DIR="release"
fi

declare -A PANELS=(
  ["builtin-panel-app-actions"]="plugins/app-actions/builtin_panel_app_actions.wasm"
  ["builtin-panel-workspace-presets"]="plugins/workspace-presets/builtin_panel_workspace_presets.wasm"
  ["builtin-panel-tool-palette"]="plugins/tool-palette/builtin_panel_tool_palette.wasm"
  ["builtin-panel-view-controls"]="plugins/view-controls/builtin_panel_view_controls.wasm"
  ["builtin-panel-panel-list"]="plugins/panel-list/builtin_panel_panel_list.wasm"
  ["builtin-panel-layers-panel"]="plugins/layers-panel/builtin_panel_layers_panel.wasm"
  ["builtin-panel-color-palette"]="plugins/color-palette/builtin_panel_color_palette.wasm"
  ["builtin-panel-pen-settings"]="plugins/pen-settings/builtin_panel_pen_settings.wasm"
  ["builtin-panel-job-progress"]="plugins/job-progress/builtin_panel_job_progress.wasm"
  ["builtin-panel-snapshot-panel"]="plugins/snapshot-panel/builtin_panel_snapshot_panel.wasm"
  ["builtin-panel-text-flow"]="plugins/text-flow/builtin_panel_text_flow.wasm"
)

echo "Ensuring wasm target is installed..."
rustup target add "$TARGET"

cd "$REPO_ROOT"

echo "Building UI panel Wasm artifacts ($PROFILE_DIR)..."
for pkg in "${!PANELS[@]}"; do
  CARGO_ARGS+=("-p" "$pkg")
done
cargo "${CARGO_ARGS[@]}"

for pkg in "${!PANELS[@]}"; do
  dest="${PANELS[$pkg]}"
  artifact_name="$(basename "$dest")"
  source="$REPO_ROOT/target/$TARGET/$PROFILE_DIR/$artifact_name"
  destination="$REPO_ROOT/$dest"

  if [ ! -f "$source" ]; then
    echo "ERROR: missing built artifact: $source" >&2
    exit 1
  fi

  mkdir -p "$(dirname "$destination")"
  cp "$source" "$destination"
  echo "Copied $artifact_name -> $dest"
done

echo "UI Wasm artifacts are ready."
