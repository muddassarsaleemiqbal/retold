; Inno Setup script for Retold Studio. Built by scripts/package.sh with /DVersion and /DSource.
[Setup]
AppName=Retold Studio
AppVersion={#Version}
AppPublisher=Retold contributors
DefaultDirName={autopf}\Retold Studio
DefaultGroupName=Retold Studio
OutputBaseFilename=Retold-Studio-{#Version}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=lowest
WizardStyle=modern
SetupIconFile=retold-studio.ico
UninstallDisplayIcon={app}\retold-studio.exe

[Files]
Source: "{#Source}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Retold Studio"; Filename: "{app}\retold-studio.exe"
Name: "{autodesktop}\Retold Studio"; Filename: "{app}\retold-studio.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Run]
Filename: "{app}\retold-studio.exe"; Description: "Launch Retold Studio"; Flags: nowait postinstall skipifsilent
