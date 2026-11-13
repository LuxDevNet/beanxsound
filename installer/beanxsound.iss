; beanxsound — single-file installer for Windows x64 and ARM64
; Build with: ISCC.exe /Qp installer\beanxsound.iss

#define AppName "beanxsound"
#define AppVersion "0.1.0"
#define AppPublisher "beanxsound"
#define AppExe "beanxsound.exe"

[Setup]
AppId={{6F1E6C2A-9F3B-4B4E-8E2A-2C7D5F0A1B23}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
DefaultDirName={localappdata}\Programs\beanxsound
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible arm64
ArchitecturesInstallIn64BitMode=x64compatible arm64
MinVersion=10.0.17763
OutputDir=..\dist
OutputBaseFilename=beanxsound-setup-{#AppVersion}
SetupIconFile=..\assets\beanxsound.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
VersionInfoVersion={#AppVersion}.0
VersionInfoDescription={#AppName} installer
VersionInfoProductName={#AppName}
VersionInfoProductVersion={#AppVersion}
Compression=lzma2/ultra64
SolidCompression=yes
LZMANumBlockThreads=2
WizardStyle=modern
WizardResizable=no
CloseApplications=yes
RestartApplications=no
AllowNoIcons=yes
DisableWelcomePage=no
LicenseFile=..\LICENSE

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"
Name: "taskbarpin"; Description: "Pin beanxsound to the taskbar"; GroupDescription: "Shortcuts:"

[Files]
Source: "..\dist\x64\{#AppExe}"; DestDir: "{app}"; DestName: "{#AppExe}"; Check: IsX64; Flags: ignoreversion
Source: "..\dist\arm64\{#AppExe}"; DestDir: "{app}"; DestName: "{#AppExe}"; Check: IsArm64; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion isreadme
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"; IconFilename: "{app}\{#AppExe}"; Comment: "Windows audio control center"
Name: "{group}\Uninstall {#AppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"; IconFilename: "{app}\{#AppExe}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; Parameters: "--pin"; Description: "Pin beanxsound to the taskbar"; Flags: runhidden waituntilterminated; Tasks: taskbarpin
Filename: "{app}\{#AppExe}"; Description: "Launch beanxsound"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{app}\{#AppExe}"; Parameters: "--unpin"; Flags: runhidden waituntilterminated; RunOnceId: "UnpinTaskbar"

[UninstallDelete]
Type: files; Name: "{userappdata}\Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar\{#AppName}.lnk"
