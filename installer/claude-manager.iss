; Inno Setup script for Claude Manager.
; Build: iscc /DAppVersion=0.1.2 /DSourceExe=..\target\release\claude-manager.exe installer\claude-manager.iss
; Per-user install (no admin prompt) into %LOCALAPPDATA%\Programs\Claude Manager, so the
; in-app updater can replace the exe in place.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceExe
  #define SourceExe "..\target\release\claude-manager.exe"
#endif

[Setup]
AppId={{6F0C5E2A-6A3B-4C1E-9D9B-2B7E5C1A9F41}
AppName=Claude Manager
AppVersion={#AppVersion}
AppVerName=Claude Manager {#AppVersion}
AppPublisher=Claude Manager contributors
AppPublisherURL=https://github.com/alihussnainrb/Claude-Manager
AppSupportURL=https://github.com/alihussnainrb/Claude-Manager/issues
AppUpdatesURL=https://github.com/alihussnainrb/Claude-Manager/releases
DefaultDirName={localappdata}\Programs\Claude Manager
DisableProgramGroupPage=yes
DisableDirPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.19041
OutputDir=..\dist
OutputBaseFilename=ClaudeManager-Setup-{#AppVersion}
SetupIconFile=..\assets\app.ico
UninstallDisplayIcon={app}\claude-manager.exe
UninstallDisplayName=Claude Manager
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=force
RestartApplications=no
LicenseFile=..\LICENSE

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "claude-manager.exe"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\Claude Manager"; Filename: "{app}\claude-manager.exe"
Name: "{userdesktop}\Claude Manager"; Filename: "{app}\claude-manager.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\claude-manager.exe"; Description: "Open Claude Manager"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; Stop the app first (otherwise it reclaims claude:// the moment cleanup restores it), then
; give claude:// links back to Claude and remove the startup entry.
Filename: "{sys}\taskkill.exe"; Parameters: "/IM claude-manager.exe /F"; Flags: runhidden waituntilterminated; RunOnceId: "Stop"
Filename: "{app}\claude-manager.exe"; Parameters: "--uninstall"; Flags: runhidden waituntilterminated; RunOnceId: "Cleanup"

[UninstallDelete]
; Leftovers from in-place updates.
Type: files; Name: "{app}\claude-manager.exe.old"
