; Voxa for Windows installer (the GPUI rebuild). Works as an update over the old Voxa:
; closes it, removes its start-at-sign-in entry, optionally uninstalls it, and on first run the new
; app imports its history, recordings, shortcut and ChatGPT sign-in.
; Build: ISCC.exe installer\voxa.iss  (after cargo build --release -p voxa-desktop -p voxa-signin)

#define AppVersion "0.3.1"
#define Bin "..\target\release"

[Setup]
AppId={{6F1C2E3A-8B7D-4C55-9E21-5A0D7C9B4F31}
AppName=Voxa
AppVersion={#AppVersion}
AppPublisher=Voxa
AppPublisherURL=https://github.com/blackscythe123/voxa
DefaultDirName={localappdata}\Programs\Voxa
DefaultGroupName=Voxa
DisableProgramGroupPage=yes
DisableDirPage=yes
; Per-user: no admin prompt to install or update.
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=Voxa-Setup-{#AppVersion}
SetupIconFile=..\apps\desktop\assets\icon.ico
UninstallDisplayIcon={app}\Voxa.exe
UninstallDisplayName=Voxa
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=no

[Files]
Source: "{#Bin}\voxa-desktop.exe"; DestDir: "{app}"; DestName: "Voxa.exe"; Flags: ignoreversion
Source: "{#Bin}\voxa-signin.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Bin}\onnxruntime.dll"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Voxa"; Filename: "{app}\Voxa.exe"
Name: "{autodesktop}\Voxa"; Filename: "{app}\Voxa.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked
Name: "removeold"; Description: "Remove the old Voxa after updating (Windows asks for permission)"; Check: OldVoxaInstalled

[Registry]
; The old app's start-at-sign-in entry: the new Voxa replaces it.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueName: "com.Voxa.app"; Flags: deletevalue
; Keep "start with Windows" if it was on, pointing at the installed copy.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Voxa Next"; ValueData: """{app}\Voxa.exe"" --background"; Check: AutostartWasOn; Flags: uninsdeletevalue

[Run]
Filename: "{app}\Voxa.exe"; Description: "Open Voxa"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM Voxa.exe"; Flags: runhidden; RunOnceId: "StopVoxa"

[UninstallDelete]
Type: filesandordirs; Name: "{app}"

[Code]
const
  OldUninstaller = '{commonpf64}\Voxa\Uninstall Voxa.exe';
var
  AutostartOn: Boolean;

function OldVoxaInstalled: Boolean;
begin
  Result := FileExists(ExpandConstant(OldUninstaller));
end;

function AutostartWasOn: Boolean;
begin
  Result := AutostartOn;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Code: Integer;
  V: String;
begin
  // Started at sign-in by either the old app or an earlier new one: keep that choice.
  AutostartOn := RegQueryStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Voxa Next', V)
    or RegQueryStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'com.Voxa.app', V);
  // Close Voxa (old and new) so files can be replaced and the shortcut is free.
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM Voxa.exe', '', SW_HIDE, ewWaitUntilTerminated, Code);
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM voxa-desktop.exe', '', SW_HIDE, ewWaitUntilTerminated, Code);
  Sleep(500);
  Result := '';
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  Code: Integer;
begin
  // Remove the old app before the new one first runs. Its data folder stays, so the new
  // Voxa can import history, recordings and sign-in from it.
  if (CurStep = ssPostInstall) and WizardIsTaskSelected('removeold') and OldVoxaInstalled then
    ShellExec('runas', ExpandConstant(OldUninstaller), '/S', '', SW_HIDE, ewWaitUntilTerminated, Code);
end;
