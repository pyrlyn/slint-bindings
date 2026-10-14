# Opens the WinUI demo on a .slint file, captures its window and fails when
# the Slint panel drew nothing. CI runs it on the weft screen; it needs a
# desktop session, which GitHub's Windows runners have.
param(
    [Parameter(Mandatory)] [string] $Exe,
    [Parameter(Mandatory)] [string] $Slint,
    [string] $Out = "smoke.png",
    [int] $Seconds = 20
)
$ErrorActionPreference = "Stop"

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class SmokeWin32 {
    public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out RECT rect);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);
}
"@

$process = Start-Process -FilePath $Exe -ArgumentList "`"$((Resolve-Path $Slint).Path)`"" -PassThru
try {
    $deadline = (Get-Date).AddSeconds($Seconds)
    while ((Get-Date) -lt $deadline) {
        $process.Refresh()
        if ($process.HasExited) { throw "the demo exited with code $($process.ExitCode)" }
        if ($process.MainWindowHandle -ne 0) { break }
        Start-Sleep -Milliseconds 500
    }
    $window = $process.MainWindowHandle
    if ($window -eq 0) { throw "the demo opened no window in $Seconds s" }
    [SmokeWin32]::SetForegroundWindow($window) | Out-Null
    # The panel loads and draws a few frames after the window shows.
    Start-Sleep -Seconds 5
    $process.Refresh()
    if ($process.HasExited) { throw "the demo exited with code $($process.ExitCode)" }

    $rect = New-Object SmokeWin32+RECT
    [SmokeWin32]::GetWindowRect($window, [ref] $rect) | Out-Null
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    $bitmap = New-Object System.Drawing.Bitmap $width, $height
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
    $bitmap.Save((Join-Path (Get-Location) $Out))

    # The demo's left column is 240 logical px of WinUI controls; sample only
    # the Slint panel right of it, below the title bar.
    $scale = [SmokeWin32]::GetDpiForWindow($window) / 96.0
    $left = [int](260 * $scale)
    $top = [int](60 * $scale)
    if ($left -ge $width - 8 -or $top -ge $height - 8) { throw "the window is too small: $width x $height" }
    $colours = @{}
    for ($x = $left; $x -lt $width - 8; $x += 4) {
        for ($y = $top; $y -lt $height - 8; $y += 4) {
            $colours[$bitmap.GetPixel($x, $y).ToArgb()] = $true
        }
    }
    $graphics.Dispose()
    $bitmap.Dispose()
    if ($colours.Count -lt 2) { throw "the Slint panel is one colour; see $Out" }
    Write-Output "The Slint panel shows $($colours.Count) colours ($width x $height window)."
}
finally {
    if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force }
}
