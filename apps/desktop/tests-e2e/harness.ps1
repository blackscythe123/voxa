# End-to-end harness for voxa-desktop on a real Windows desktop. Dot-source it, then call the functions.
# It moves the real mouse and keyboard: run only when nobody is using the PC.
Add-Type -AssemblyName System.Windows.Forms, System.Drawing, UIAutomationClient, UIAutomationTypes
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class W {
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, UIntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint f, UIntPtr e);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  public static string Title(IntPtr h){ var s=new StringBuilder(256); GetWindowText(h,s,256); return s.ToString(); }
  public static void Click(int x,int y){ SetCursorPos(x,y); System.Threading.Thread.Sleep(80); mouse_event(2,0,0,0,UIntPtr.Zero); mouse_event(4,0,0,0,UIntPtr.Zero); }
  public static void Hotkey(){ keybd_event(0x11,0,0,UIntPtr.Zero); keybd_event(0x12,0,0,UIntPtr.Zero); keybd_event(0x20,0,0,UIntPtr.Zero);
    keybd_event(0x20,0,2,UIntPtr.Zero); keybd_event(0x12,0,2,UIntPtr.Zero); keybd_event(0x11,0,2,UIntPtr.Zero); }
}
"@
[W]::SetProcessDPIAware() | Out-Null

$Spike = "C:\projects\voxa\apps\desktop\tests-e2e"
$Exe = "C:\projects\voxa\target\debug\voxa-desktop.exe"
$TestFile = "$env:TEMP\voxa-spike-notepad.txt"

function Get-Fg { $h=[W]::GetForegroundWindow(); "$h `"$([W]::Title($h))`"" }

function Start-Target {
  Start-Process powershell.exe -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$Spike\target.ps1`"" -WindowStyle Minimized
  Start-Sleep 3
  [W]::Click(600, 450)   # click into the text box, like a user would
  Start-Sleep -Milliseconds 400
  "target ready; foreground = $(Get-Fg)"
}

function Get-TargetText { "target text: [" + (Get-Content "$env:TEMP\voxa-spike-target.txt" -Raw) + "]" }

function Start-Voxa([string]$fake = "", [string]$session = "") {
  $env:VOXA_FAKE = $fake; $env:VOXA_SESSION = $session
  Start-Process $Exe -WorkingDirectory $Spike
  Start-Sleep 3
  Remove-Item Env:VOXA_FAKE, Env:VOXA_SESSION -ErrorAction SilentlyContinue
  "voxa started (fake=$fake session=$session); foreground = $(Get-Fg)"
}

function Show-Log([int]$n = 12) { Get-Content "$env:APPDATA\voxa-next\voxa-desktop.log" | Select-Object -Last $n }

function Save-Screen([string]$name) {
  $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
  $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
  $g = [System.Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
  $bmp.Save("$Spike\$name.png"); $g.Dispose(); $bmp.Dispose()
  "saved $name.png ($($b.Width)x$($b.Height))"
}

function Stop-Test {
  Get-Process voxa-desktop -ErrorAction SilentlyContinue | Stop-Process -Force
  Get-CimInstance Win32_Process -Filter "Name='powershell.exe'" | Where-Object { $_.CommandLine -like "*target.ps1*" } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }
  "stopped spike and target; clipboard now: [$(Get-Clipboard)]"
}
