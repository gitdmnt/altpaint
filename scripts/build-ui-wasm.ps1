param(
    [switch]$Release,
    [switch]$SkipTargetInstall
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $repoRoot

try {
    $target = 'wasm32-unknown-unknown'
    $profileDir = if ($Release) { 'release' } else { 'debug' }
    $cargoArgs = @('build', '--target', $target)
    if ($Release) {
        $cargoArgs += '--release'
    }

    $panelPackages = @(
        @{ Package = 'builtin-panel-app-actions'; Destination = 'crates/builtin-panels/app-actions/builtin_panel_app_actions.wasm' },
        @{ Package = 'builtin-panel-workspace-presets'; Destination = 'crates/builtin-panels/workspace-presets/builtin_panel_workspace_presets.wasm' },
        @{ Package = 'builtin-panel-tool-palette'; Destination = 'crates/builtin-panels/tool-palette/builtin_panel_tool_palette.wasm' },
        @{ Package = 'builtin-panel-view-controls'; Destination = 'crates/builtin-panels/view-controls/builtin_panel_view_controls.wasm' },
        @{ Package = 'builtin-panel-panel-list'; Destination = 'crates/builtin-panels/panel-list/builtin_panel_panel_list.wasm' },
        @{ Package = 'builtin-panel-layers-panel'; Destination = 'crates/builtin-panels/layers-panel/builtin_panel_layers_panel.wasm' },
        @{ Package = 'builtin-panel-color-palette'; Destination = 'crates/builtin-panels/color-palette/builtin_panel_color_palette.wasm' },
        @{ Package = 'builtin-panel-pen-settings'; Destination = 'crates/builtin-panels/pen-settings/builtin_panel_pen_settings.wasm' },
        @{ Package = 'builtin-panel-job-progress'; Destination = 'crates/builtin-panels/job-progress/builtin_panel_job_progress.wasm' },
        @{ Package = 'builtin-panel-snapshot-panel'; Destination = 'crates/builtin-panels/snapshot-panel/builtin_panel_snapshot_panel.wasm' },
        @{ Package = 'builtin-panel-text-flow'; Destination = 'crates/builtin-panels/text-flow/builtin_panel_text_flow.wasm' },
        @{ Package = 'builtin-panel-workspace-layout'; Destination = 'crates/builtin-panels/workspace-layout/builtin_panel_workspace_layout.wasm' }
    )

    if (-not $SkipTargetInstall) {
        Write-Host 'Ensuring wasm target is installed...'
        rustup target add $target
    }

    foreach ($panel in $panelPackages) {
        $cargoArgs += @('-p', $panel.Package)
    }

    Write-Host "Building UI panel Wasm artifacts ($profileDir)..."
    & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE"
    }

    foreach ($panel in $panelPackages) {
        $artifactName = Split-Path $panel.Destination -Leaf
        $source = Join-Path $repoRoot "target/$target/$profileDir/$artifactName"
        $destination = Join-Path $repoRoot $panel.Destination
        $destinationDir = Split-Path -Parent $destination
        if (-not (Test-Path $source)) {
            throw "missing built artifact: $source"
        }
        if (-not (Test-Path $destinationDir)) {
            New-Item -ItemType Directory -Path $destinationDir | Out-Null
        }
        Copy-Item $source $destination -Force
        Write-Host "Copied $artifactName -> $($panel.Destination)"
    }

    Write-Host 'UI Wasm artifacts are ready.'
}
finally {
    Pop-Location
}