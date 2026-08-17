[CmdletBinding()]
param(
    [string]$Version,

    [ValidateSet('Release', 'Debug')]
    [string]$Configuration = 'Release'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$projectPath = Join-Path $repositoryRoot 'src\Hyperkey.App\Hyperkey.App.csproj'
$versionPropsPath = Join-Path $repositoryRoot 'Directory.Build.props'
$publishPath = Join-Path $repositoryRoot 'publish\win-x64'
$installerScriptPath = Join-Path $repositoryRoot 'installer\Hyperkey.iss'

if (-not (Test-Path -LiteralPath $projectPath -PathType Leaf)) {
    throw "The Hyperkey app project was not found: $projectPath"
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

$dotnet = Get-Command dotnet -ErrorAction SilentlyContinue
if ($null -eq $dotnet) {
    throw 'The .NET SDK was not found on PATH.'
}

Write-Host "Publishing Hyperkey $Version ($Configuration, win-x64)..."
& $dotnet.Source publish $projectPath `
    --configuration $Configuration `
    --runtime win-x64 `
    --self-contained true `
    --output $publishPath `
    --property:Platform=x64 `
    --property:Version=$Version

$publishedExecutable = Join-Path $publishPath 'Hyperkey.App.exe'
if (-not (Test-Path -LiteralPath $publishedExecutable -PathType Leaf)) {
    throw "The published executable was not created: $publishedExecutable"
}

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

$innoSetupCandidates = $innoSetupCandidates |
    Where-Object { Test-Path -LiteralPath $_ -PathType Leaf }

$innoSetup = $innoSetupCandidates | Select-Object -First 1
if ($null -eq $innoSetup) {
    throw 'Inno Setup 6 was not found. Install it or add ISCC.exe to PATH.'
}

Write-Host "Building installer with $innoSetup..."
& $innoSetup "/DAppVersion=$Version" $installerScriptPath

$installerPath = Join-Path $repositoryRoot "publish\installer\Hyperkey-Setup-$Version.exe"
if (-not (Test-Path -LiteralPath $installerPath -PathType Leaf)) {
    throw "The installer was not created: $installerPath"
}

Write-Host "Installer created: $installerPath"
