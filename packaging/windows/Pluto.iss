#define PlutoVersion GetEnv('PLUTO_VERSION')
#define PlutoBinary GetEnv('PLUTO_BINARY')
#define PlutoPublisher GetEnv('PLUTO_SIGNING_COMPANY_NAME')
#if PlutoVersion == ""
  #error PLUTO_VERSION must be set by package_windows_installer.ps1
#endif
#if PlutoBinary == ""
  #error PLUTO_BINARY must be set by package_windows_installer.ps1
#endif
#if PlutoPublisher == ""
  #error PLUTO_SIGNING_COMPANY_NAME must match the release certificate
#endif

[Setup]
AppId={{A88A8770-06E2-4A63-9F0C-0872E33CF11D}
AppName=Pluto
AppVersion={#PlutoVersion}
AppVerName=Pluto {#PlutoVersion}
AppPublisher={#PlutoPublisher}
AppPublisherURL=https://github.com/amxcodes/little-world-pluto
AppSupportURL=https://github.com/amxcodes/little-world-pluto/issues
DefaultDirName={localappdata}\Programs\Pluto
DefaultGroupName=Pluto
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
OutputBaseFilename=Pluto-Setup-v{#PlutoVersion}-windows-x64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern dynamic hidebevels
WizardBackColor=#F6F6F7
WizardBackColorDynamicDark=#0B0D11
WizardImageFile=
WizardSmallImageFile=Assets\Square150x150Logo.png
WizardSmallImageBackColor=#F6F6F7
WizardSmallImageBackColorDynamicDark=#0B0D11
SetupIconFile=Assets\Pluto.ico
UninstallDisplayIcon={app}\Pluto.exe
CloseApplications=yes
RestartApplications=no
SignedUninstaller=yes
SignTool=pluto
VersionInfoVersion={#PlutoVersion}.0
VersionInfoProductName=Pluto
VersionInfoDescription=Pluto installer

[Files]
Source: "{#PlutoBinary}"; DestDir: "{app}"; DestName: "Pluto.exe"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Pluto"; Filename: "{app}\Pluto.exe"; IconFilename: "{app}\Pluto.exe"

[Run]
Filename: "{app}\Pluto.exe"; Description: "Launch Pluto"; Flags: nowait postinstall skipifsilent

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "LittleWorldIsland"; ValueData: """{app}\Pluto.exe"""; Check: ExistingStartupEnabled

[Code]
function ExistingStartupEnabled(): Boolean;
begin
  Result := RegValueExists(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'LittleWorldIsland');
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Command: String;
begin
  if CurUninstallStep <> usPostUninstall then Exit;
  if RegQueryStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'LittleWorldIsland', Command) and
     (CompareText(Command, '"' + ExpandConstant('{app}\Pluto.exe') + '"') = 0) then
    RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'LittleWorldIsland');
end;
