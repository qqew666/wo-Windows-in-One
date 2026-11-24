#ifndef AppVersion
  #error Pass /DAppVersion with the version from app/Cargo.toml.
#endif

[Setup]
AppId={{3C394A86-0844-49F6-A16B-62061DEA9842}
AppName=Two Windows in One
AppVersion={#AppVersion}
AppPublisher=qqew666
AppPublisherURL=https://twi.wtowindowszuz.chatgpt.site/
AppSupportURL=https://github.com/qqew666/wo-Windows-in-One
AppUpdatesURL=https://github.com/qqew666/wo-Windows-in-One/releases/latest
DefaultDirName={localappdata}\Programs\Two Windows in One
DefaultGroupName=Two Windows in One
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
DisableProgramGroupPage=yes
UsePreviousAppDir=yes
AppMutex=Local\TwoWindowsInOne-3C394A86
CloseApplications=no
RestartApplications=no
UninstallDisplayIcon={app}\icon.ico
WizardStyle=modern
SetupIconFile=icon.ico
OutputDir=..\..\dist
OutputBaseFilename=Two-Windows-in-One-Setup
Compression=lzma2
SolidCompression=yes

[Languages]
Name: "ru"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "..\target\x86_64-pc-windows-msvc\release\TwoWindowsInOne.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "icon.ico"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Two Windows in One"; Filename: "{app}\TwoWindowsInOne.exe"; IconFilename: "{app}\icon.ico"
Name: "{userdesktop}\Two Windows in One"; Filename: "{app}\TwoWindowsInOne.exe"; IconFilename: "{app}\icon.ico"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; Flags: unchecked

[Run]
Filename: "{app}\TwoWindowsInOne.exe"; Description: "{cm:LaunchProgram,Two Windows in One}"; Flags: nowait postinstall skipifsilent
