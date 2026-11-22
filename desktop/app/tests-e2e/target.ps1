# A stand-in "WhatsApp": a plain window with a text box. Mirrors its text to a file so the test can read it.
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
$out = "$env:TEMP\voxa-spike-target.txt"
Set-Content -Path $out -Value "" -NoNewline
$f = New-Object System.Windows.Forms.Form
$f.Text = "Voxa Spike Target"
$f.Add_Shown({ $f.WindowState = "Normal"; $f.Activate() })
$f.StartPosition = "Manual"; $f.Location = New-Object System.Drawing.Point(200, 200); $f.Size = New-Object System.Drawing.Size(900, 500)
$tb = New-Object System.Windows.Forms.TextBox
$tb.Multiline = $true; $tb.Dock = "Fill"; $tb.Font = New-Object System.Drawing.Font("Segoe UI", 14)
$f.Controls.Add($tb)
$t = New-Object System.Windows.Forms.Timer; $t.Interval = 300
$t.Add_Tick({ Set-Content -Path $out -Value $tb.Text -NoNewline })
$t.Start()
[System.Windows.Forms.Application]::Run($f)
