# Fetch onnxruntime.dll 1.22.0 (needed by voxa-vad / silence trimming) from Microsoft's official
# NuGet package, check its hash, and put it in vendor/onnxruntime. The desktop build copies it next
# to voxa-desktop.exe.
$ErrorActionPreference = "Stop"
$sha = "579b636403983254346a5c1d80bd28f1519cd1e284cd204f8d4ff41f8d711559"
$dst = Join-Path $PSScriptRoot "..\..\vendor\onnxruntime"
New-Item -ItemType Directory -Force $dst | Out-Null
$work = Join-Path $env:TEMP ("ort-" + [guid]::NewGuid())
New-Item -ItemType Directory $work | Out-Null
$pkg = Join-Path $work "ort.zip"
Invoke-WebRequest "https://api.nuget.org/v3-flatcontainer/microsoft.ml.onnxruntime/1.22.0/microsoft.ml.onnxruntime.1.22.0.nupkg" -OutFile $pkg -UseBasicParsing
Expand-Archive $pkg -DestinationPath $work
$dll = Join-Path $work "runtimes\win-x64\native\onnxruntime.dll"
if ((Get-FileHash $dll -Algorithm SHA256).Hash.ToLower() -ne $sha) { throw "onnxruntime.dll hash mismatch" }
Copy-Item $dll $dst -Force
Remove-Item $work -Recurse -Force
"onnxruntime.dll ready in $dst"
