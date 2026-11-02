; ZIYNAT — Windows installer (Inno Setup 6).
; Built by .github/workflows/windows.yml:
;   ISCC /DAppVersion=<v> /DSourceDir=<dist\ZIYNAT> /DIconFile=<ico> /O<out> ziynat.iss

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\..\..\dist\ZIYNAT"
#endif
#ifndef IconFile
  #define IconFile "..\..\..\assets\app-icon\photocraft.ico"
#endif

[Setup]
AppId={{8E6B6C1A-2F0B-4E7D-9C8A-5A1D2E3F4B60}
AppName=ZIYNAT
AppVersion={#AppVersion}
AppVerName=ZIYNAT {#AppVersion}
AppPublisher=ZIYNAT
DefaultDirName={autopf}\ZIYNAT
DefaultGroupName=ZIYNAT
DisableProgramGroupPage=yes
OutputBaseFilename=ZIYNAT-{#AppVersion}-windows-x64-setup
SetupIconFile={#IconFile}
UninstallDisplayIcon={app}\ZIYNAT.exe
Compression=lzma2/max
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
WizardStyle=modern
ChangesAssociations=yes
LicenseFile={#SourceDir}\LICENSE-MIT

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "assoc"; Description: "Open .psd, .pcraft, .ora files with ZIYNAT"; GroupDescription: "File types:"; Flags: unchecked

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\ZIYNAT"; Filename: "{app}\ZIYNAT.exe"
Name: "{autodesktop}\ZIYNAT"; Filename: "{app}\ZIYNAT.exe"; Tasks: desktopicon

[Registry]
Root: HKA; Subkey: "Software\Classes\ZIYNAT.Document"; ValueType: string; ValueName: ""; ValueData: "ZIYNAT image"; Flags: uninsdeletekey; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\ZIYNAT.Document\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\ZIYNAT.exe,0"; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\ZIYNAT.Document\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\ZIYNAT.exe"" ""%1"""; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\.pcraft\OpenWithProgids"; ValueType: string; ValueName: "ZIYNAT.Document"; ValueData: ""; Flags: uninsdeletevalue; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\.psd\OpenWithProgids"; ValueType: string; ValueName: "ZIYNAT.Document"; ValueData: ""; Flags: uninsdeletevalue; Tasks: assoc
Root: HKA; Subkey: "Software\Classes\.ora\OpenWithProgids"; ValueType: string; ValueName: "ZIYNAT.Document"; ValueData: ""; Flags: uninsdeletevalue; Tasks: assoc

[Run]
Filename: "{app}\ZIYNAT.exe"; Description: "{cm:LaunchProgram,ZIYNAT}"; Flags: nowait postinstall skipifsilent
