#Requires -Version 5.1
param([switch] $Smoke)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repo = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
Import-Module (Join-Path $repo 'scripts/setup-path.psm1') -Force

function Assert-Equal($Actual, $Expected) {
    if ($Actual -cne $Expected) { throw "Expected [$Expected], got [$Actual]" }
}
function Assert-True([bool] $Condition, [string] $Message) {
    if (-not $Condition) { throw $Message }
}

# Parse every PowerShell entry point, including Windows PowerShell 5.1 syntax.
foreach ($file in @('scripts/setup.ps1', 'scripts/setup-path.psm1')) {
    $tokens = $null
    $errors = $null
    [void] [Management.Automation.Language.Parser]::ParseFile((Join-Path $repo $file), [ref] $tokens, [ref] $errors)
    Assert-Equal $errors.Count 0
}
$existing = 'C:\unrelated tools;%USERPROFILE%\custom tools;;D:\other;'
$bin = "C:\rayengine cli's\bin"
$added = Add-PathEntry $existing $bin
Assert-Equal $added "$existing;$bin"
Assert-Equal (Add-PathEntry $added $bin) $added
Assert-Equal (Remove-PathEntries $added @($bin)) $existing
Assert-Equal (Add-PathEntry '' $bin) $bin
Assert-Equal (Remove-PathEntries $bin @($bin)) ''
Assert-Equal (Add-PathEntry 'C:/RayEngine CLI/bin\;C:\other' 'c:\rayengine cli\BIN') 'C:/RayEngine CLI/bin\;C:\other'
Assert-True (Test-PathEntry "$bin;C:\other" $bin) 'Expected existing bin entry'
Assert-True (-not (Test-PathEntry "${bin}-extra" $bin)) 'Substring is not a PATH entry'
$env:RAYENGINE_TEST_HOME = 'C:\custom cargo home'
Assert-Equal (Add-PathEntry '%RAYENGINE_TEST_HOME%\bin;C:\other' 'c:\custom cargo home\bin') '%RAYENGINE_TEST_HOME%\bin;C:\other'
Remove-Item Env:RAYENGINE_TEST_HOME
Write-Host 'Windows PATH unit tests passed.'

if (-not $Smoke) { return }
if ($env:OS -ne 'Windows_NT') { throw 'Windows smoke must run on Windows.' }

$temp = Join-Path ([IO.Path]::GetTempPath()) ("rayengine setup " + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $temp | Out-Null
$root = Join-Path $temp "cli's install root"
$originalProcess = $env:Path
$originalLocalAppData = $env:LOCALAPPDATA
$originalInstallRoot = $env:CARGO_INSTALL_ROOT
$key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
$hadPath = 'Path' -in $key.GetValueNames()
$originalUser = Get-UserPath
$originalKind = if ($hadPath) { $key.GetValueKind('Path') } else { [Microsoft.Win32.RegistryValueKind]::ExpandString }
$key.Dispose()
try {
    # Isolate ownership state, and keep this test's user PATH fully reversible.
    $env:LOCALAPPDATA = Join-Path $temp 'local app data'
    $baseline = "$originalUser;%LOCALAPPDATA%\unrelated tools;"
    Set-UserPath $baseline
    $env:CARGO_INSTALL_ROOT = $root
    $setup = Join-Path $repo 'scripts/setup.ps1'
    $powershell = (Get-Process -Id $PID).Path
    $mockTools = Join-Path $temp 'mock tools'
    New-Item -ItemType Directory -Path $mockTools | Out-Null
    $env:Path = $mockTools
    function Assert-SetupFailure([string] $ExpectedMessage) {
        $stdout = Join-Path $temp 'failure.stdout'
        $stderr = Join-Path $temp 'failure.stderr'
        $process = Start-Process -FilePath $powershell -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', ('"' + $setup + '"'), '-DebugBuild') -Wait -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
        Assert-True ($process.ExitCode -ne 0) 'Expected setup failure'
        $message = (Get-Content -LiteralPath $stdout -Raw) + (Get-Content -LiteralPath $stderr -Raw)
        Assert-True ($message -like "*$ExpectedMessage*") "Missing error hint: $message"
        Assert-Equal (Get-UserPath) $baseline
        Assert-True (-not (Test-Path -LiteralPath (Join-Path $env:LOCALAPPDATA 'rayengine/setup-path.json'))) 'Failed install changed ownership'
    }
    Assert-SetupFailure 'Missing rustc'
    '@echo rustc 1.88.0 (fixture)' | Set-Content -LiteralPath (Join-Path $mockTools 'rustc.cmd') -Encoding ASCII
    '@echo cargo 1.89.0 (fixture)' | Set-Content -LiteralPath (Join-Path $mockTools 'cargo.cmd') -Encoding ASCII
    Assert-SetupFailure 'Rust 1.89+ required'
    @'
@echo off
if "%1"=="-vV" (echo host: x86_64-pc-windows-msvc) else (echo rustc 1.89.0)
'@ | Set-Content -LiteralPath (Join-Path $mockTools 'rustc.cmd') -Encoding ASCII
    @'
@echo off
if "%1"=="--version" (echo cargo 1.89.0) else (exit /b 17)
'@ | Set-Content -LiteralPath (Join-Path $mockTools 'cargo.cmd') -Encoding ASCII
    Assert-SetupFailure 'Cargo installation failed'
    $env:Path = $originalProcess
    & $setup -DebugBuild
    Assert-Equal $LASTEXITCODE 0
    $firstPath = Get-UserPath
    $state = Join-Path $env:LOCALAPPDATA 'rayengine/setup-path.json'
    $firstState = Get-Content -LiteralPath $state -Raw
    # Set-Location does not update .NET's process CurrentDirectory.
    Push-Location $temp
    try {
        & $setup -DebugBuild -Root "cli's install root"
        Assert-Equal $LASTEXITCODE 0
    } finally { Pop-Location }
    Assert-Equal (Get-UserPath) $firstPath
    Assert-Equal (Get-Content -LiteralPath $state -Raw) $firstState
    Assert-True (Test-Path -LiteralPath (Join-Path $root 'bin/rayengine.exe')) 'Installed binary missing'

    # A spawned process normally inherits this process's environment. Build the
    # PATH a new Explorer terminal receives, from persisted machine/user values.
    $env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::ExpandEnvironmentVariables((Get-UserPath))
    $childScript = Join-Path $temp 'fresh terminal.ps1'
    @'
$ErrorActionPreference = 'Stop'
$command = Get-Command rayengine -CommandType Application
if ($command.Source -ne $env:RAYENGINE_EXPECTED_BINARY) { throw "Wrong rayengine: $($command.Source)" }
& rayengine --help
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
'@ | Set-Content -LiteralPath $childScript -Encoding UTF8
    $env:RAYENGINE_EXPECTED_BINARY = Join-Path $root 'bin/rayengine.exe'
    $powershell = (Get-Process -Id $PID).Path
    & $powershell -NoProfile -File $childScript
    Assert-Equal $LASTEXITCODE 0

    & $setup -RemovePath
    Assert-Equal (Get-UserPath) $baseline
    Assert-True (-not (Test-Path -LiteralPath $state)) 'State was not removed'
    & $setup -RemovePath
    Assert-Equal (Get-UserPath) $baseline

    # A PATH edit made during a lengthy Cargo build must survive setup.
    $duringBuild = Join-Path $temp 'edit path during build.ps1'
    @'
$key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
try {
    $value = $key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    $key.SetValue('Path', "$value;D:\added during build", $key.GetValueKind('Path'))
} finally { $key.Dispose() }
'@ | Set-Content -LiteralPath $duringBuild -Encoding UTF8
    $env:RAYENGINE_MOCK_POWERSHELL = $powershell
    $env:RAYENGINE_MOCK_PATH_SCRIPT = $duringBuild
    @'
@echo off
if "%1"=="--version" (echo cargo 1.89.0 & exit /b 0)
"%RAYENGINE_MOCK_POWERSHELL%" -NoProfile -File "%RAYENGINE_MOCK_PATH_SCRIPT%"
exit /b %errorlevel%
'@ | Set-Content -LiteralPath (Join-Path $mockTools 'cargo.cmd') -Encoding ASCII
    $env:Path = $mockTools
    & $setup -Root $root -DebugBuild
    Assert-Equal $LASTEXITCODE 0
    $concurrentPath = "$baseline;D:\added during build"
    Assert-Equal (Get-UserPath) (Add-PathEntry $concurrentPath (Join-Path $root 'bin'))
    & $setup -RemovePath
    Assert-Equal (Get-UserPath) $concurrentPath
    $env:Path = $originalProcess
    Set-UserPath $baseline

    # A preconfigured user entry is never claimed or removed by setup.
    Set-UserPath (Add-PathEntry $baseline (Join-Path $root 'bin'))
    $preconfigured = Get-UserPath
    & $setup -Root $root -DebugBuild
    Assert-Equal $LASTEXITCODE 0
    Assert-True (-not (Test-Path -LiteralPath $state)) 'Setup claimed a preexisting entry'
    & $setup -RemovePath
    Assert-Equal (Get-UserPath) $preconfigured

    $env:Path = $originalProcess
    & cargo uninstall rayengine-cli --root $root
    Assert-Equal $LASTEXITCODE 0
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $root 'bin/rayengine.exe'))) 'Uninstall did not remove binary'
    Write-Host 'Windows install, fresh session, idempotence and reversal smoke passed.'
} finally {
    $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
    try {
        if ($hadPath) { $key.SetValue('Path', $originalUser, $originalKind) }
        else { $key.DeleteValue('Path', $false) }
    } finally { $key.Dispose() }
    $env:Path = $originalProcess
    $env:LOCALAPPDATA = $originalLocalAppData
    $env:CARGO_INSTALL_ROOT = $originalInstallRoot
    Remove-Item Env:RAYENGINE_EXPECTED_BINARY, Env:RAYENGINE_MOCK_POWERSHELL, Env:RAYENGINE_MOCK_PATH_SCRIPT -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $temp -Recurse -Force
}
