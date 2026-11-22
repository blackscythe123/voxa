# Old (Electron) vs new (GPUI) Voxa: hotkey-to-strip latency, a screen recording, and RAM.
# Drives the real desktop. Run only when nobody is using the PC.
. "$PSScriptRoot\harness.ps1"
Add-Type @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class Vis {
  public delegate bool EnumProc(IntPtr h, IntPtr p);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc f, IntPtr p);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT r);
  // Is any small (strip-sized) window of these processes visible?
  public static bool StripVisible(int[] pids) {
    var set = new HashSet<uint>(); foreach (var p in pids) set.Add((uint)p);
    bool found = false;
    EnumWindows((h, _) => {
      uint pid; GetWindowThreadProcessId(h, out pid);
      if (!set.Contains(pid) || !IsWindowVisible(h)) return true;
      RECT r; GetWindowRect(h, out r);
      int w = r.R - r.L, hgt = r.B - r.T;
      if (w > 100 && w < 1300 && hgt > 30 && hgt < 260) { found = true; return false; }
      return true;
    }, IntPtr.Zero);
    return found;
  }
}
"@

function Send-Keys([byte[]]$down) {
  foreach ($k in $down) { [W]::keybd_event($k, 0, 0, [UIntPtr]::Zero) }
  [Array]::Reverse($down); foreach ($k in $down) { [W]::keybd_event($k, 0, 2, [UIntPtr]::Zero) }
}

function Measure-Show([string]$proc, [byte[]]$keys, [int]$timeoutMs = 10000) {
  $pids = @(Get-Process $proc | ForEach-Object Id)
  $sw = [Diagnostics.Stopwatch]::StartNew()
  Send-Keys $keys
  while ($sw.ElapsedMilliseconds -lt $timeoutMs) {
    if ([Vis]::StripVisible($pids)) { return $sw.ElapsedMilliseconds }
    Start-Sleep -Milliseconds 5
  }
  return -1
}

function Mem([string]$name) {
  $ps = Get-Process $name -ErrorAction SilentlyContinue
  if (-not $ps) { return "$name not running" }
  "{0}: {1} process(es), working set {2:N0} MB, private {3:N0} MB" -f $name, $ps.Count, (($ps | Measure-Object WorkingSet64 -Sum).Sum/1MB), (($ps | Measure-Object PrivateMemorySize64 -Sum).Sum/1MB)
}

$CTRL = 0x11; $ALT = 0x12; $SPACE = 0x20
$ffmpeg = (Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages" -Recurse -Filter ffmpeg.exe | Select-Object -First 1).FullName
$video = "$PSScriptRoot\old-vs-new.mp4"
Remove-Item $video -ErrorAction SilentlyContinue

"--- idle RAM ---"; Mem "Voxa"; Mem "voxa-desktop"
Start-Target | Out-Null
$rec = Start-Process $ffmpeg -ArgumentList "-y -loglevel error -f gdigrab -framerate 30 -i desktop -vf scale=1280:-2 -c:v libx264 -preset ultrafast -pix_fmt yuv420p -t 32 `"$video`"" -PassThru -WindowStyle Hidden
Start-Sleep 2

"--- OLD Voxa (Electron, Ctrl+Space) ---"
$old = Measure-Show "Voxa" @($CTRL, $SPACE)
"strip visible after: $old ms"
Start-Sleep 3; "while recording: $(Mem 'Voxa')"
Send-Keys @($CTRL, $SPACE)
Start-Sleep 8

[W]::Click(600, 450); Start-Sleep -Milliseconds 400
"--- NEW Voxa (GPUI, Ctrl+Alt+Space) ---"
$new = Measure-Show "voxa-desktop" @($CTRL, $ALT, $SPACE)
"strip visible after: $new ms"
Start-Sleep 3; "while recording: $(Mem 'voxa-desktop')"
Send-Keys @($CTRL, $ALT, $SPACE)
Start-Sleep 7

$rec.WaitForExit(15000) | Out-Null
"video: $video ($([math]::Round((Get-Item $video).Length/1MB,1)) MB)"
Get-TargetText
