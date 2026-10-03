[CmdletBinding()]
param(
    # The upgrade check builds and installs a second installer, so it roughly doubles
    # the runtime. Skip it while iterating locally; CI always runs the full path.
    [switch]$SkipUpgrade
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repositoryRoot = Split-Path -Parent $PSScriptRoot
$packageScriptPath = Join-Path $repositoryRoot 'scripts\package-installer.ps1'
$versionPropsPath = Join-Path $repositoryRoot 'Directory.Build.props'
$installerDirectory = Join-Path $repositoryRoot 'publish\installer'

$installDirectory = Join-Path $env:LOCALAPPDATA 'Programs\Hyperkey'
$installedExecutable = Join-Path $installDirectory 'Hyperkey.App.exe'
$uninstallerPath = Join-Path $installDirectory 'unins000.exe'
$startMenuShortcut = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Hyperkey.lnk'
$appDataDirectory = Join-Path $env:LOCALAPPDATA 'Hyperkey'
$settingsPath = Join-Path $appDataDirectory 'settings.json'
$runKeyPath = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$runValueName = 'Hyperkey'

$workDirectory = Join-Path ([System.IO.Path]::GetTempPath()) 'hyperkey-installer-verification'
$backupDirectory = Join-Path $workDirectory 'appdata-backup'

function Write-Step {
    param([string]$Message)

    Write-Host "==> $Message" -ForegroundColor Cyan
}

function Write-Check {
    param([string]$Message)

    Write-Host "    ok  $Message" -ForegroundColor DarkGray
}

function Assert-Condition {
    param(
        [bool]$Condition,
        [string]$Message
    )

    if (-not $Condition) {
        throw "Verification failed: $Message"
    }

    Write-Check $Message
}

function Assert-FileExists {
    param(
        [string]$Path,
        [string]$Message
    )

    Assert-Condition -Condition (Test-Path -LiteralPath $Path -PathType Leaf) `
        -Message "$Message ($Path)"
}

function Assert-PathMissing {
    param(
        [string]$Path,
        [string]$Message
    )

    Assert-Condition -Condition (-not (Test-Path -LiteralPath $Path)) `
        -Message "$Message ($Path)"
}

function Get-StartupRegistration {
    $key = Get-Item -LiteralPath $runKeyPath -ErrorAction SilentlyContinue
    if ($null -eq $key) {
        return $null
    }

    $value = $key.GetValue($runValueName, $null)
    if ($null -eq $value) {
        return $null
    }

    return [string]$value
}

function Set-StartupRegistration {
    param([string]$Value)

    if ($null -eq $Value) {
        Remove-ItemProperty -LiteralPath $runKeyPath -Name $runValueName -ErrorAction SilentlyContinue
        return
    }

    New-Item -Path $runKeyPath -Force | Out-Null
    Set-ItemProperty -LiteralPath $runKeyPath -Name $runValueName -Value $Value
}

function Get-InstalledVersion {
    $versionInfo = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($installedExecutable)
    if ([string]::IsNullOrWhiteSpace($versionInfo.ProductVersion)) {
        return ''
    }

    # The build appends "+<commit>" to the informational version; AppVersion.cs
    # splits on the same character before displaying it.
    return ($versionInfo.ProductVersion -split '\+', 2)[0]
}

function Get-LogTail {
    param(
        [string]$Path,
        [int]$Lines = 25
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return "No log file was written to $Path."
    }

    return ((Get-Content -LiteralPath $Path -Tail $Lines) -join [Environment]::NewLine)
}

function Invoke-Setup {
    param(
        [string]$Path,
        [string]$Activity
    )

    $logPath = Join-Path $workDirectory "inno-$Activity.log"
    $arguments = @(
        '/VERYSILENT'
        '/SUPPRESSMSGBOXES'
        '/NORESTART'
        '/SP-'
        ('/LOG="{0}"' -f $logPath)
    )

    $process = Start-Process -FilePath $Path -ArgumentList $arguments -Wait -PassThru
    if ($process.ExitCode -ne 0) {
        throw "$Activity failed with exit code $($process.ExitCode). Inno Setup log tail:`n$((Get-LogTail $logPath))"
    }
}

function Invoke-Uninstaller {
    $logPath = Join-Path $workDirectory 'inno-uninstall.log'
    $arguments = @(
        '/VERYSILENT'
        '/SUPPRESSMSGBOXES'
        '/NORESTART'
        '/SP-'
        ('/LOG="{0}"' -f $logPath)
    )

    $process = Start-Process -FilePath $uninstallerPath -ArgumentList $arguments -Wait -PassThru
    if ($process.ExitCode -ne 0) {
        throw "Uninstall failed with exit code $($process.ExitCode). Inno Setup log tail:`n$((Get-LogTail $logPath))"
    }
}

function Build-Installer {
    param([string]$Version)

    Write-Step "Building installer $Version"
    # Out-Host keeps the publish log visible without letting it become the return value.
    # package-installer.ps1 signals failure by throwing, which propagates here, so the
    # Assert-FileExists below is the only guard this needs.
    & $packageScriptPath -Version $Version | Out-Host

    $setupPath = Join-Path $installerDirectory "Hyperkey-Setup-$Version.exe"
    Assert-FileExists -Path $setupPath -Message "Installer $Version was produced"
    return $setupPath
}

function Stop-ProcessIfRunning {
    param([System.Diagnostics.Process]$Process)

    if ($null -eq $Process) {
        return
    }

    try {
        if ($Process.HasExited) {
            return
        }

        [void]$Process.CloseMainWindow()
        if (-not $Process.WaitForExit(10000)) {
            # The tray app has no main window to close politely in every state.
            Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
            [void]$Process.WaitForExit(10000)
        }
    }
    catch {
        Write-Host "    warn  could not stop process $($Process.Id): $($_.Exception.Message)" -ForegroundColor Yellow
    }
    finally {
        $Process.Dispose()
    }
}

function Test-StartupAndSingleInstance {
    Write-Step 'Starting the installed app and checking the single-instance guard'

    $process = Start-Process -FilePath $installedExecutable -PassThru
    Start-Sleep -Seconds 6

    $process.Refresh()
    Assert-Condition -Condition (-not $process.HasExited) `
        -Message 'The installed app stays running after launch'

    $second = Start-Process -FilePath $installedExecutable -PassThru
    $secondExited = $second.WaitForExit(30000)
    $second.Dispose()

    Assert-Condition -Condition $secondExited `
        -Message 'A second instance exits instead of opening another window'

    $process.Refresh()
    Assert-Condition -Condition (-not $process.HasExited) `
        -Message 'The first instance is unaffected by the second launch'

    Stop-ProcessIfRunning -Process $process
}

function Set-UserState {
    Write-Step 'Seeding settings and launch-at-login as a user would leave them'

    New-Item -ItemType Directory -Path $appDataDirectory -Force | Out-Null
    $settings = @'
{
  "schemaVersion": 2,
  "enabled": true,
  "trigger": "F7",
  "outputModifiers": [
    "Control",
    "Alt"
  ],
  "launchAtStartup": true,
  "launchToTray": true
}
'@
    Set-Content -LiteralPath $settingsPath -Value $settings -Encoding utf8NoBOM

    # The app only writes this value when the user flips the setting, so seed it
    # directly. The uninstaller is what has to clean it up.
    Set-StartupRegistration -Value ('"{0}"' -f $installedExecutable)

    Assert-FileExists -Path $settingsPath -Message 'settings.json is in place'
    Assert-Condition -Condition ($null -ne (Get-StartupRegistration)) `
        -Message 'Launch-at-login registration is in place'
}

function Test-Upgrade {
    param(
        [string]$FromVersion,
        [string]$ToVersion
    )

    Write-Step "Upgrading $FromVersion to $ToVersion over an existing install"

    $setupPath = Build-Installer -Version $ToVersion
    Invoke-Setup -Path $setupPath -Activity "upgrade-$ToVersion"

    Assert-Condition -Condition ((Get-InstalledVersion) -eq $ToVersion) `
        -Message "The installed executable reports version $ToVersion"

    $preservedTrigger = (Get-Content -LiteralPath $settingsPath -Raw | ConvertFrom-Json).trigger
    Assert-Condition -Condition ($preservedTrigger -eq 'F7') `
        -Message 'Existing settings survive the upgrade'

    $programsDirectory = Split-Path -Parent $startMenuShortcut
    $hyperkeyShortcuts = @(Get-ChildItem -LiteralPath $programsDirectory -Filter 'Hyperkey*.lnk')
    Assert-Condition -Condition ($hyperkeyShortcuts.Count -eq 1) `
        -Message 'The upgrade leaves a single Start Menu shortcut'
}

function Test-CleanUninstall {
    Write-Step 'Uninstalling and checking that nothing is left behind'

    Invoke-Uninstaller

    Assert-PathMissing -Path $installedExecutable -Message 'The executable is removed'
    Assert-PathMissing -Path $uninstallerPath -Message 'The uninstaller is removed'
    Assert-PathMissing -Path $startMenuShortcut -Message 'The Start Menu shortcut is removed'
    Assert-PathMissing -Path $installDirectory -Message 'The install directory is removed'
    Assert-PathMissing -Path $appDataDirectory -Message 'The settings directory is removed'
    Assert-Condition -Condition ($null -eq (Get-StartupRegistration)) `
        -Message 'Launch-at-login registration is removed'
}

if (-not (Test-Path -LiteralPath $packageScriptPath -PathType Leaf)) {
    throw "The packaging script was not found: $packageScriptPath"
}

if (Test-Path -LiteralPath $installDirectory) {
    throw "Hyperkey is already installed at $installDirectory. Uninstall it before running this script so real user data is not touched."
}

$runningInstances = @(Get-Process -Name 'Hyperkey.App' -ErrorAction SilentlyContinue)
if ($runningInstances.Count -gt 0) {
    throw "Hyperkey is running. Quit it from the tray and run this script again; the single-instance check needs a clean session."
}

[xml]$versionDocument = Get-Content -LiteralPath $versionPropsPath -Raw
$baseVersion = $versionDocument.SelectSingleNode('/Project/PropertyGroup/VersionPrefix').InnerText.Trim()
$versionParts = $baseVersion.Split('.')
$upgradeVersion = "$($versionParts[0]).$($versionParts[1]).$([int]$versionParts[2] + 1)"

$hadExistingAppData = Test-Path -LiteralPath $appDataDirectory
$hadExistingShortcut = Test-Path -LiteralPath $startMenuShortcut
$savedStartupRegistration = Get-StartupRegistration
$startedVerification = $false

New-Item -ItemType Directory -Path $workDirectory -Force | Out-Null

# Uninstalling deletes %LOCALAPPDATA%\Hyperkey outright, so keep a copy to put back.
if ($hadExistingAppData) {
    Copy-Item -LiteralPath $appDataDirectory -Destination $backupDirectory -Recurse -Force
}

try {
    $startedVerification = $true

    $setupPath = Build-Installer -Version $baseVersion
    Invoke-Setup -Path $setupPath -Activity "install-$baseVersion"

    Write-Step 'Checking what a clean install puts on disk'
    Assert-FileExists -Path $installedExecutable -Message 'The executable is installed'
    Assert-FileExists -Path $uninstallerPath -Message 'The uninstaller is installed'
    Assert-FileExists -Path $startMenuShortcut -Message 'The Start Menu shortcut is installed'
    Assert-Condition -Condition ((Get-InstalledVersion) -eq $baseVersion) `
        -Message "The installed executable reports version $baseVersion"
    Assert-Condition -Condition ((Get-StartupRegistration) -eq $savedStartupRegistration) `
        -Message 'Installing leaves launch-at-login registration untouched'

    Test-StartupAndSingleInstance
    Set-UserState

    if ($SkipUpgrade) {
        Write-Step 'Skipping the upgrade check (-SkipUpgrade)'
    }
    else {
        Test-Upgrade -FromVersion $baseVersion -ToVersion $upgradeVersion
    }

    Test-CleanUninstall

    Write-Host "`nInstaller verification passed." -ForegroundColor Green
}
finally {
    $verificationFailed = $true

    Get-Process -Name 'Hyperkey.App' -ErrorAction SilentlyContinue |
        ForEach-Object { Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue }

    if ($startedVerification) {
        # Cleanup runs guarded. A throw here would replace the real "Verification failed"
        # exception with a confusing cleanup error and hide which check actually broke.
        try {
            if (Test-Path -LiteralPath $installDirectory) {
                Remove-Item -LiteralPath $installDirectory -Recurse -Force -ErrorAction SilentlyContinue
            }

            # A failed run can leave the shortcut behind if it never reached the uninstaller.
            if (-not $hadExistingShortcut) {
                Get-ChildItem -LiteralPath (Split-Path -Parent $startMenuShortcut) -Filter 'Hyperkey*.lnk' -ErrorAction SilentlyContinue |
                    ForEach-Object { Remove-Item -LiteralPath $_.FullName -Force -ErrorAction SilentlyContinue }
            }

            # The uninstaller deletes the settings directory, so put the machine back the
            # way it was found before reporting anything.
            if (Test-Path -LiteralPath $appDataDirectory) {
                Remove-Item -LiteralPath $appDataDirectory -Recurse -Force -ErrorAction SilentlyContinue
            }

            if ($hadExistingAppData) {
                Copy-Item -LiteralPath $backupDirectory -Destination $appDataDirectory -Recurse -Force
            }

            if ($null -eq $savedStartupRegistration) {
                Remove-ItemProperty -LiteralPath $runKeyPath -Name $runValueName -ErrorAction SilentlyContinue
            }
            else {
                New-Item -Path $runKeyPath -Force | Out-Null
                Set-ItemProperty -LiteralPath $runKeyPath -Name $runValueName -Value $savedStartupRegistration
            }

            $verificationFailed = $false
        }
        catch {
            Write-Host "    warn  cleanup could not fully restore this machine: $($_.Exception.Message)" -ForegroundColor Yellow
            Write-Host "    warn  your previous settings are backed up at $backupDirectory" -ForegroundColor Yellow
        }
    }

    if ($verificationFailed) {
        # Keep the Inno logs and the settings backup for diagnosis and recovery.
        Write-Host "    logs and settings backup kept at $workDirectory" -ForegroundColor Yellow
    }
    else {
        Remove-Item -LiteralPath $workDirectory -Recurse -Force -ErrorAction SilentlyContinue
    }
}
