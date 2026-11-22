# Drive and capture the Voxa main window. Dot-source, then:
#   Start-VoxaUi -Data <dir> [-Fake ok]   launch on a throwaway data folder
#   Click-Ui x y                          click at logical (1100x760 design) coordinates
#   Shot-Ui name                          save a screenshot of the window to $Shots\name.png
#   Stop-VoxaUi
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class UiWin {
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowW(string c, string t);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L,T,R,B; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X,Y; }
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, UIntPtr e);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint f, UIntPtr e);
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  // The biggest visible window of the process: the main window (the strip is small).
  public static IntPtr Main(uint pid) {
    IntPtr best = IntPtr.Zero; long area = 0;
    EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p); RECT r;
      if (p == pid && IsWindowVisible(h) && GetWindowRect(h, out r)) { long a = (long)(r.R-r.L)*(r.B-r.T); if (a > area && (r.B-r.T) > 300) { area = a; best = h; } }
      return true; }, IntPtr.Zero);
    return best;
  }
}
"@
[UiWin]::SetProcessDPIAware() | Out-Null
$Shots = "$env:TEMP\voxa-shots"; New-Item -ItemType Directory -Force $Shots | Out-Null
$Ff = (Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages" -Recurse -Filter ffmpeg.exe | Select-Object -First 1).FullName
$UiExe = "C:\projects\voxa\rebuild\target\release\voxa-desktop.exe"

# The main window may live in its own process (--window): search every Voxa process.
function Get-Ui {
  foreach ($p in @(Get-Process voxa-desktop, Voxa -ErrorAction SilentlyContinue)) { $h = [UiWin]::Main([uint32]$p.Id); if ($h -ne [IntPtr]::Zero) { return $h } }
  return [IntPtr]::Zero
}

function Start-VoxaUi([string]$Data, [string]$Fake = "ok") {
  Get-Process voxa-desktop -ErrorAction SilentlyContinue | Stop-Process -Force
  Start-Sleep -Milliseconds 400
  $env:VOXA_DATA = $Data; $env:VOXA_FAKE = $Fake
  $script:UiProc = Start-Process $UiExe -WorkingDirectory $Data -PassThru
  Remove-Item Env:VOXA_DATA, Env:VOXA_FAKE -ErrorAction SilentlyContinue
  for ($i = 0; $i -lt 50 -and (Get-Ui) -eq [IntPtr]::Zero; $i++) { Start-Sleep -Milliseconds 100 }
  Start-Sleep -Milliseconds 700
  "window: $(Get-Ui)"
}

# Windows only lets the foreground app raise windows; a tapped Alt lifts that lock.
function Front-Ui {
  $h = Get-Ui
  [UiWin]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero); [UiWin]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
  [UiWin]::SetForegroundWindow($h) | Out-Null
  Start-Sleep -Milliseconds 250
  $h
}

function Click-Ui([double]$x, [double]$y) {
  $h = Front-Ui
  if ($h -eq [IntPtr]::Zero) { throw "Voxa window not found: not clicking" }
  $s = [UiWin]::GetDpiForWindow($h) / 96.0
  $p = New-Object UiWin+POINT; $p.X = [int]($x * $s); $p.Y = [int]($y * $s)
  [UiWin]::ClientToScreen($h, [ref]$p) | Out-Null
  [UiWin]::SetCursorPos($p.X, $p.Y) | Out-Null; Start-Sleep -Milliseconds 60
  [UiWin]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero); [UiWin]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 350
}

function Type-Ui([string]$text) {
  foreach ($c in $text.ToUpper().ToCharArray()) {
    $vk = [byte][char]$c
    [UiWin]::keybd_event($vk, 0, 0, [UIntPtr]::Zero); [UiWin]::keybd_event($vk, 0, 2, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 40
  }
  Start-Sleep -Milliseconds 250
}

function Shot-Ui([string]$name) {
  $h = Front-Ui; Start-Sleep -Milliseconds 200; $r = New-Object UiWin+RECT; [UiWin]::GetWindowRect($h, [ref]$r) | Out-Null
  & $Ff -y -loglevel error -f gdigrab -offset_x $r.L -offset_y $r.T -video_size "$($r.R - $r.L)x$($r.B - $r.T)" -i desktop -frames:v 1 "$Shots\$name.png"
  "$Shots\$name.png"
}

function Stop-VoxaUi { Get-Process voxa-desktop -ErrorAction SilentlyContinue | Stop-Process -Force }
