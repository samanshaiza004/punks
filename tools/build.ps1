[CmdletBinding()]
param(
    [string]$Odin,
    [switch]$Release
)

$ErrorActionPreference = 'Stop'
$RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$AlicornRoot = Join-Path $RepoRoot 'vendor/alicorn'
$AlicornTools = Join-Path $AlicornRoot 'tools'
. (Join-Path $AlicornTools 'common.ps1')

Initialize-AlicornMsvcEnvironment
$Odin = Resolve-AlicornOdin -Requested $Odin
$AccessKit = Get-AlicornAccessKit

$Profile = 'debug'
$CargoArgs = @('build', '-p', 'punks-caliber')
if ($Release) {
    $Profile = 'release'
    $CargoArgs += '--release'
}

Push-Location $RepoRoot
try {
    & cargo @CargoArgs
    if ($LASTEXITCODE -ne 0) {
        throw "Cargo failed to build punks-caliber (exit $LASTEXITCODE)."
    }

    $CargoOutput = Join-Path $RepoRoot "target/$Profile"
    $CaliberImportLibrary = Join-Path $CargoOutput 'punks_caliber.dll.lib'
    $CaliberRuntime = Join-Path $CargoOutput 'punks_caliber.dll'
    foreach ($Artifact in @($CaliberImportLibrary, $CaliberRuntime)) {
        if (-not (Test-Path -LiteralPath $Artifact -PathType Leaf)) {
            throw "Expected Caliber build artifact is missing: '$Artifact'."
        }
    }

    $OdinRoot = Split-Path -Parent $Odin
    $SDLRuntime = Join-Path $OdinRoot 'vendor/sdl3/SDL3.dll'
    if (-not (Test-Path -LiteralPath $SDLRuntime -PathType Leaf)) {
        throw "SDL3.dll was not found at '$SDLRuntime'. Use a complete Odin distribution with vendor/sdl3."
    }
    if ((Get-Item -LiteralPath $SDLRuntime).Length -lt 100000) {
        throw "SDL3.dll at '$SDLRuntime' appears to be a Git LFS pointer, not a runtime."
    }

    $OutputDirectory = Join-Path $RepoRoot "target/windows-x86_64/$Profile"
    New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
    $Executable = Join-Path $OutputDirectory 'punks.exe'
    $LinkerFlags = $AccessKit.LinkerFlags
    $PreviousLib = $env:LIB
    if ([string]::IsNullOrWhiteSpace($PreviousLib)) {
        $env:LIB = $CargoOutput
    } else {
        $env:LIB = "$CargoOutput;$PreviousLib"
    }
    $BuildArgs = @(
        'build',
        'punks-standalone/odin',
        "-collection:alicorn=$AlicornRoot",
        "-extra-linker-flags:$LinkerFlags",
        "-out:$Executable"
    )
    if ($Release) { $BuildArgs += '-o:speed' }
    try {
        & $Odin @BuildArgs
        if ($LASTEXITCODE -ne 0) {
            throw "Odin failed to build the Punks native frontend (exit $LASTEXITCODE)."
        }
    } finally {
        if ([string]::IsNullOrWhiteSpace($PreviousLib)) {
            Remove-Item Env:LIB -ErrorAction SilentlyContinue
        } else {
            $env:LIB = $PreviousLib
        }
    }

    Copy-Item -LiteralPath $SDLRuntime -Destination (Join-Path $OutputDirectory 'SDL3.dll') -Force
    Copy-Item -LiteralPath $CaliberRuntime -Destination (Join-Path $OutputDirectory 'punks_caliber.dll') -Force

    Write-Host "Built runnable Windows x64 folder: $OutputDirectory"
    Write-Host "Executable: $Executable"
    Write-Host 'Runtime files: SDL3.dll, punks_caliber.dll'
} finally {
    Pop-Location
}