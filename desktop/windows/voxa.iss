; Voxa for Windows installer (the GPUI rebuild). Works as an update over the old Voxa:
; closes it, removes its start-at-sign-in entry, optionally uninstalls it, and on first run the new
; app imports its history, recordings, shortcut and ChatGPT sign-in.
; Build: ISCC.exe /DAppVersion=<version> desktop\windows\voxa.iss  (after cargo build --release -p voxa-desktop -p voxa-signin)
;
; Automatic updates: the tray runs this installer with /VERYSILENT ... /VOXAUPDATE=<from version>
; /MERGETASKS="!removeold" once its install gate holds (desktop/app/src/update.rs). Then Setup waits
; for Voxa to exit by itself (it never kills a take), never asks for UAC, and starts Voxa again
; with --background --updated, after a success and after a rollback alike.

; The version comes from Cargo.toml (CI passes /DAppVersion). This default must equal the
; workspace version; the lint release-version-sync checks it.
#ifndef AppVersion
#define AppVersion "0.4.7"
#endif
#define Bin "..\..\target\release"

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
OutputDir=..\..\target\dist
OutputBaseFilename=Voxa-Setup-{#AppVersion}
SetupIconFile=..\app\assets\icon.ico
UninstallDisplayIcon={app}\Voxa.exe
UninstallDisplayName=Voxa
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Voxa closes itself before an automatic update; never AppMutex (in silent mode it aborts on the
; tray that launched Setup) and never RestartApplications.
CloseApplications=no
SetupLogging=yes

[Files]
Source: "{#Bin}\voxa-desktop.exe"; DestDir: "{app}"; DestName: "Voxa.exe"; Flags: ignoreversion
Source: "{#Bin}\voxa-signin.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Bin}\onnxruntime.dll"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
; The ID lets Voxa post Windows notifications (it matches SetCurrentProcessExplicitAppUserModelID).
Name: "{autoprograms}\Voxa"; Filename: "{app}\Voxa.exe"; AppUserModelID: "Voxa.Desktop"
Name: "{autodesktop}\Voxa"; Filename: "{app}\Voxa.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked
Name: "removeold"; Description: "Remove the old Voxa after updating (Windows asks for permission)"; Check: OldVoxaInstalled and not WizardSilent

[Registry]
; The old app's start-at-sign-in entry: the new Voxa replaces it.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueName: "com.Voxa.app"; Flags: deletevalue
; Keep "start with Windows" if it was on, pointing at the installed copy.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Voxa Next"; ValueData: """{app}\Voxa.exe"" --background"; Check: AutostartWasOn; Flags: uninsdeletevalue

; Voxa registers its notification name and icon here at start; removed on uninstall.
Root: HKCU; Subkey: "Software\Classes\AppUserModelId\Voxa.Desktop"; Flags: uninsdeletekey dontcreatekey

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

// True when the tray started this Setup as an automatic update (/VOXAUPDATE=<from version>).
function IsAutoUpdate: Boolean;
begin
  Result := ExpandConstant('{param:VOXAUPDATE|}') <> '';
end;

// An automatic update: Voxa quits by itself once its gate holds. Wait up to 20 s for both of its
// mutexes to go before falling back to taskkill, so a take is never cut off.
procedure WaitForVoxaToExit;
var
  I: Integer;
begin
  for I := 1 to 80 do
  begin
    if not CheckForMutexes('Local\VoxaNextSingleInstance,Local\VoxaNextWindow') then
      Exit;
    Sleep(250);
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Code: Integer;
  V: String;
begin
  // Started at sign-in by either the old app or an earlier new one: keep that choice.
  AutostartOn := RegQueryStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Voxa Next', V)
    or RegQueryStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'com.Voxa.app', V);
  if IsAutoUpdate then
    WaitForVoxaToExit;
  // Close Voxa (old and new) so files can be replaced and the shortcut is free. After an
  // automatic update's wait this finds nothing; it stays as the fallback.
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM Voxa.exe', '', SW_HIDE, ewWaitUntilTerminated, Code);
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM voxa-desktop.exe', '', SW_HIDE, ewWaitUntilTerminated, Code);
  // The sign-in helper is installed too; a running copy locks its file and fails a silent install.
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM voxa-signin.exe', '', SW_HIDE, ewWaitUntilTerminated, Code);
  Sleep(500);
  Result := '';
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  Code: Integer;
begin
  // Remove the old app before the new one first runs. Its data folder stays, so the new
  // Voxa can import history, recordings and sign-in from it.
  // Never in silent mode: 'runas' raises a UAC prompt that steals focus.
  if (CurStep = ssPostInstall) and (not WizardSilent) and WizardIsTaskSelected('removeold') and OldVoxaInstalled then
    ShellExec('runas', ExpandConstant(OldUninstaller), '/S', '', SW_HIDE, ewWaitUntilTerminated, Code);
end;

// An automatic update starts Voxa again in the background (no window, no focus), whether the
// install succeeded or rolled back, so the hotkey always comes back. [Run]'s postinstall entry is
// skipped in silent mode.
procedure DeinitializeSetup;
var
  Code: Integer;
begin
  if not IsAutoUpdate then
    Exit;
  try
    if FileExists(ExpandConstant('{app}\Voxa.exe')) then
      Exec(ExpandConstant('{app}\Voxa.exe'), '--background --updated', '', SW_HIDE, ewNoWait, Code);
  except
    // {app} is unknown if Setup stopped before it was set; nothing to start then.
  end;
end;
