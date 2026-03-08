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
        @{ Package = 'builtin-panel-app-actions'; Destination = 'apps/desktop/ui/panels/app-actions/builtin_panel_app_actions.wasm' },
        @{ Package = 'builtin-panel-tool-palette'; Destination = 'apps/desktop/ui/panels/tool-palette/builtin_panel_tool_palette.wasm' },
        @{ Package = 'builtin-panel-layers-panel'; Destination = 'apps/desktop/ui/panels/layers-panel/builtin_panel_layers_panel.wasm' },
        @{ Package = 'builtin-panel-color-palette'; Destination = 'apps/desktop/ui/panels/color-palette/builtin_panel_color_palette.wasm' },
        @{ Package = 'builtin-panel-job-progress'; Destination = 'apps/desktop/ui/panels/job-progress/builtin_panel_job_progress.wasm' },
        @{ Package = 'builtin-panel-snapshot-panel'; Destination = 'apps/desktop/ui/panels/snapshot-panel/builtin_panel_snapshot_panel.wasm' }
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

    $phase6Wat = Join-Path $repoRoot 'apps/desktop/ui/phase6-sample.wat'
    $phase6Wasm = Join-Path $repoRoot 'apps/desktop/ui/phase6-sample.wasm'
    Copy-Item $phase6Wat $phase6Wasm -Force
    Write-Host 'Copied phase6-sample.wat -> apps/desktop/ui/phase6-sample.wasm'
    Write-Host 'UI Wasm artifacts are ready.'
}
finally {
    Pop-Location
}