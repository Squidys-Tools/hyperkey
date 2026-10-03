The .NET 8 SDK is installed per-user at ~\.dotnet.

Other `dotnet` installs on this machine are on PATH and a `MSBuildSDKsPath` environment
variable may point at one of them. That shadows the per-user 8.0 SDK and makes every build
fail with `MSB4062 ... could not be loaded from the assembly ... net10.0`. Clear the variable
and put the 8.0 SDK first before building:

```powershell
$env:MSBuildSDKsPath = $null
$env:DOTNET_ROOT = "$env:USERPROFILE\.dotnet"
$env:PATH = "$env:DOTNET_ROOT;$env:PATH"
```

Inno Setup 6 is required for anything under `scripts\`. It is installed per-user at
`%LOCALAPPDATA%\Programs\Inno Setup 6`, which is not on PATH by default.
