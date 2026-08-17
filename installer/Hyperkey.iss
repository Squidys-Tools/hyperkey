#ifndef AppVersion
  #error AppVersion must be supplied by scripts/package-installer.ps1
#endif

#define AppName "Hyperkey"
#define AppPublisher "Hyperkey"
#define AppExeName "Hyperkey.App.exe"
#define PublishDir "..\publish\win-x64"

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
CloseApplications=yes
RestartApplications=no
UninstallDisplayIcon={app}\{#AppExeName}

[Files]
Source: "{#PublishDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExeName}"; WorkingDir: "{app}"; Comment: "Open Hyperkey settings"

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
