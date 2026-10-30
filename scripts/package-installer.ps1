[CmdletBinding()]
param(
    [string]$Version,

    [ValidateSet('Release', 'Debug')]
    [string]$Configuration = 'Release'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$rustWorkspacePath = Join-Path $repositoryRoot 'rust'
$versionPropsPath = Join-Path $repositoryRoot 'Directory.Build.props'
$publishPath = Join-Path $repositoryRoot 'publish\app'
$installerScriptPath = Join-Path $repositoryRoot 'installer\Hyperkey.iss'

if (-not (Test-Path -LiteralPath (Join-Path $rustWorkspacePath 'Cargo.toml') -PathType Leaf)) {
    throw "The Rust workspace was not found: $rustWorkspacePath"
}

if (-not (Test-Path -LiteralPath $installerScriptPath -PathType Leaf)) {
    throw "The Inno Setup script was not found: $installerScriptPath"
}

if ([string]::IsNullOrWhiteSpace($Version)) {
    if (-not (Test-Path -LiteralPath $versionPropsPath -PathType Leaf)) {
        throw "The shared version file was not found: $versionPropsPath"
    }

    [xml]$versionDocument = Get-Content -LiteralPath $versionPropsPath -Raw
    $versionNode = $versionDocument.SelectSingleNode('/Project/PropertyGroup/VersionPrefix')
    $Version = $versionNode.InnerText.Trim()
}

if ($Version -notmatch '^\d+\.\d+\.\d+$') {
    throw "Version must use three numeric components, for example 0.1.0. Received: $Version"
}

$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if ($null -eq $cargo) {
    throw 'The Rust toolchain was not found on PATH.'
}

$cargoArguments = @('build', '--package', 'hyperkey-app')
$profileDirectory = 'debug'
if ($Configuration -eq 'Release') {
    $cargoArguments += '--release'
    $profileDirectory = 'release'
}

Write-Host "Building hyperkey-app $Version ($Configuration)..."
Push-Location $rustWorkspacePath
try {
    & $cargo.Source @cargoArguments
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE."
    }
}
finally {
    Pop-Location
}

$builtExecutable = Join-Path $rustWorkspacePath "target\$profileDirectory\hyperkey-app.exe"
if (-not (Test-Path -LiteralPath $builtExecutable -PathType Leaf)) {
    throw "The built executable was not created: $builtExecutable"
}

# The setup ships the Rust binary under the C# executable name. Combined with the
# unchanged AppId and install directory, existing installs upgrade in place and
# their shortcuts and launch-at-login value keep pointing at a real file.
if (Test-Path -LiteralPath $publishPath) {
    Remove-Item -LiteralPath $publishPath -Recurse -Force
}
New-Item -ItemType Directory -Path $publishPath -Force | Out-Null
$stagedExecutable = Join-Path $publishPath 'Hyperkey.App.exe'
Copy-Item -LiteralPath $builtExecutable -Destination $stagedExecutable

$innoSetupCandidates = @()
$innoSetupCommand = Get-Command ISCC.exe -ErrorAction SilentlyContinue
if ($null -ne $innoSetupCommand) {
    $innoSetupCandidates += $innoSetupCommand.Source
}

foreach ($programFilesRoot in @(${env:ProgramFiles(x86)}, $env:ProgramFiles)) {
    if (-not [string]::IsNullOrWhiteSpace($programFilesRoot)) {
        $innoSetupCandidates += Join-Path $programFilesRoot 'Inno Setup 6\ISCC.exe'
    }
}

# Inno Setup's per-user install lives under the local app data folder, which is where
# it lands when a machine-wide install is not available.
$innoSetupCandidates += Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'

$innoSetupCandidates = $innoSetupCandidates |
    Where-Object { Test-Path -LiteralPath $_ -PathType Leaf }

$innoSetup = $innoSetupCandidates | Select-Object -First 1
if ($null -eq $innoSetup) {
    throw 'Inno Setup 6 was not found. Install it or add ISCC.exe to PATH.'
}

Write-Host "Building installer with $innoSetup..."
& $innoSetup "/DAppVersion=$Version" $installerScriptPath
if ($LASTEXITCODE -ne 0) {
    throw "ISCC failed with exit code $LASTEXITCODE."
}

$installerPath = Join-Path $repositoryRoot "publish\installer\Hyperkey-Setup-$Version.exe"
if (-not (Test-Path -LiteralPath $installerPath -PathType Leaf)) {
    throw "The installer was not created: $installerPath"
}

Write-Host "Installer created: $installerPath"
