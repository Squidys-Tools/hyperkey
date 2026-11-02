#ifndef AppVersion
  #error AppVersion must be supplied by scripts/package-installer.ps1
#endif

#define AppName "Hyperkey"
#define AppPublisher "Hyperkey"
; The shipped binary is the Rust hyperkey-app build, staged as Hyperkey.App.exe
; by scripts/package-installer.ps1 so existing installs upgrade in place.
#define AppExeName "Hyperkey.App.exe"
#define PublishDir "..\publish\app"

[Setup]
AppId={{9C2D0E61-8C79-4F56-9DB4-4DB4F0B6F4E3}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
DefaultDirName={localappdata}\Programs\Hyperkey
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
OutputDir=..\publish\installer
OutputBaseFilename=Hyperkey-Setup-{#AppVersion}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
SetupIconFile=favicon.ico
CloseApplications=yes
RestartApplications=no
; The Rust binary embeds no icon resource, so the shortcut and uninstaller
; icons point at the .ico shipped next to it.
UninstallDisplayIcon={app}\Hyperkey.ico

; Clear the old payload before copying: the C# install left ~200 files that
; Inno would never remove, and their presence would also keep the uninstaller
; from removing {app} cleanly.
[InstallDelete]
Type: filesandordirs; Name: "{app}\*"

[Files]
Source: "{#PublishDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "favicon.ico"; DestDir: "{app}"; DestName: "Hyperkey.ico"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExeName}"; WorkingDir: "{app}"; IconFilename: "{app}\Hyperkey.ico"; Comment: "Open Hyperkey settings"

[Run]
Filename: "{app}\{#AppExeName}"; Description: "Launch Hyperkey"; WorkingDir: "{app}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: filesandordirs; Name: "{localappdata}\Hyperkey"

[Code]
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
    RegDeleteValue(
      HKEY_CURRENT_USER,
      'Software\Microsoft\Windows\CurrentVersion\Run',
      'Hyperkey');
end;
