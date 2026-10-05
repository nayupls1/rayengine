#Requires -Version 5.1
<#
.SYNOPSIS
Install this checkout's CLI and persist its binary directory in the Windows user PATH.
.DESCRIPTION
Root precedence: -Root, CARGO_INSTALL_ROOT, CARGO_HOME, then USERPROFILE\.cargo.
Cargo install.root config is explicitly overridden. Release builds are the default.
-RemovePath reverses only entries this setup added; binaries remain installed.
.EXAMPLE
& .\scripts\setup.ps1 -Root "$env:LOCALAPPDATA\rayengine cli"
.EXAMPLE
& .\scripts\setup.ps1 -RemovePath
#>
[CmdletBinding()]
param(
    [string] $Root,
    [switch] $DebugBuild,
    [switch] $RemovePath
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
try {
    if ($env:OS -ne 'Windows_NT') { throw 'Use bash scripts/setup.sh on Linux/macOS.' }
    Import-Module (Join-Path $PSScriptRoot 'setup-path.psm1') -Force
    $stateFile = Join-Path $env:LOCALAPPDATA 'rayengine\setup-path.json'
    function Read-PathOwnership([string] $File) {
        $added = @()
        if (Test-Path -LiteralPath $File) {
            $state = Get-Content -LiteralPath $File -Raw | ConvertFrom-Json
            if ($state.version -ne 1) { throw "Unknown PATH state format in $File." }
            $added = @($state.addedPaths)
            foreach ($entry in $added) {
                if ($entry -isnot [string] -or [string]::IsNullOrEmpty($entry)) {
                    throw "Invalid PATH state in $File; repair it before retrying."
                }
            }
        }
        return $added
    }
    $owned = @(Read-PathOwnership $stateFile)
    $userPath = Get-UserPath
    if ($RemovePath) {
        $updated = Remove-PathEntries $userPath $owned
        if ($updated -ne $userPath) { Set-UserPath $updated }
        $env:Path = Remove-PathEntries $env:Path $owned
        if (Test-Path -LiteralPath $stateFile) { Remove-Item -LiteralPath $stateFile }
        Write-Host 'Removed setup-owned user PATH entries. Open a new terminal.'
        return
    }

    if (-not $Root) {
        if ($env:CARGO_INSTALL_ROOT) { $Root = $env:CARGO_INSTALL_ROOT }
        elseif ($env:CARGO_HOME) { $Root = $env:CARGO_HOME }
        else { $Root = Join-Path $env:USERPROFILE '.cargo' }
    }
    # Resolve against PowerShell's location; .NET CurrentDirectory may still
    # point at the directory where this PowerShell process was launched.
    $Root = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Root)
    if ($Root.IndexOfAny([char[]] ";`r`n") -ge 0) { throw 'Install root must not contain semicolons or newlines.' }
    foreach ($tool in @('rustc', 'cargo')) {
        if (-not (Get-Command $tool -CommandType Application -ErrorAction SilentlyContinue)) {
            throw "Missing $tool. Install Rust 1.89+ from https://rustup.rs, reopen the terminal, and retry."
        }
    }
    $rustVersion = & rustc --version
    if ($LASTEXITCODE -ne 0 -or $rustVersion -notmatch '^rustc (\d+)\.(\d+)\.') {
        throw 'Cannot read Rust version. Repair/select your Rust toolchain with rustup.'
    }
    if ([int] $Matches[1] -lt 1 -or ([int] $Matches[1] -eq 1 -and [int] $Matches[2] -lt 89)) {
        throw "Rust 1.89+ required; found $rustVersion. Run rustup update stable."
    }
    & cargo --version
    if ($LASTEXITCODE -ne 0) { throw 'Cargo failed. Repair/select your Rust toolchain with rustup.' }
    $hostTarget = @(& rustc -vV | Where-Object { $_ -match '^host: ' })
    if ($LASTEXITCODE -ne 0 -or $hostTarget.Count -ne 1) { throw 'Cannot detect Rust host target; check rustc -vV.' }
    $hostTarget = $hostTarget[0].Substring(6)
    New-Item -ItemType Directory -Force -Path $Root | Out-Null
    $Root = (Get-Item -LiteralPath $Root).FullName
    $binDir = Join-Path $Root 'bin'
    $checkout = Split-Path $PSScriptRoot -Parent
    $cargoArgs = @('install', '--locked', '--path', (Join-Path $checkout 'crates\rayengine-cli'), '--root', $Root, '--bin', 'rayengine', '--target', $hostTarget)
    if ($DebugBuild) { $cargoArgs += '--debug' }
    Write-Host "Installing checkout CLI into $binDir"
    & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) {
        throw 'Cargo installation failed; see diagnostics above. Check the linker (MSVC Build Tools for MSVC Rust), network, checkout lockfile and root permissions, then retry. No PATH changes were made.'
    }
    $binary = Join-Path $binDir 'rayengine.exe'
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw "Cargo succeeded but $binary is missing. No PATH changes were made." }
    & $binary --help | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Installed CLI cannot run. No PATH changes were made.' }

    # Cargo can take minutes; preserve PATH/state edits made during the build.
    $userPath = Get-UserPath
    $owned = @(Read-PathOwnership $stateFile)
    # Do not record an entry already owned by the user's or machine's config.
    $machinePath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    if (-not (Test-PathEntry $userPath $binDir) -and -not (Test-PathEntry $machinePath $binDir)) {
        $updated = Add-PathEntry $userPath $binDir
        $owned += $binDir
        New-Item -ItemType Directory -Force -Path (Split-Path $stateFile -Parent) | Out-Null
        # Save ownership before updating PATH so a partially failed setup is reversible.
        @{ version = 1; addedPaths = @($owned) } | ConvertTo-Json | Set-Content -LiteralPath $stateFile -Encoding UTF8
        Set-UserPath $updated
    }
    $env:Path = Add-PathEntry $env:Path $binDir
    & $binary --version
    Write-Host 'PATH is active in this PowerShell session.'
    Write-Host 'Close all terminal windows, open a NEW terminal from Start, and run: rayengine --help'
    Write-Host 'If it resolves another installation, inspect it with: Get-Command rayengine -All'
    $quotedRoot = $Root.Replace("'", "''")
    Write-Host "Uninstall the binary: cargo uninstall rayengine-cli --root '$quotedRoot'"
} catch {
    Write-Error "rayengine setup: $_" -ErrorAction Continue
    exit 1
}
