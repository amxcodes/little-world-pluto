#Requires -Version 5.1
param([switch]$Light, [switch]$Capsule)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type -ReferencedAssemblies System.Windows.Forms -TypeDefinition @'
public class IslandContrastBackdrop : System.Windows.Forms.Form {
    protected override bool ShowWithoutActivation { get { return true; } }
    [System.Runtime.InteropServices.DllImport("user32.dll")]
    public static extern bool SetProcessDpiAwarenessContext(System.IntPtr context);
}
'@
[void][IslandContrastBackdrop]::SetProcessDpiAwarenessContext([IntPtr](-4))
$root = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$renderer = 'wgpu'
$theme = if ($Light) { 'light' } else { 'dark' }
$shape = if ($Capsule) { 'capsule' } else { 'notch' }
$backdrop = [IslandContrastBackdrop]::new()
$backdrop.FormBorderStyle = 'None'
$backdrop.StartPosition = 'Manual'
$backdrop.Bounds = [Windows.Forms.Screen]::PrimaryScreen.Bounds
$backdrop.BackColor = [Drawing.Color]::FromArgb(70,120,150)
$backdrop.TopMost = $true
try {
    $backdrop.Show()
    [Windows.Forms.Application]::DoEvents()
    $arguments = @('-Edge')
    if ($Light) { $arguments += '-Light' }
    if ($Capsule) { $arguments += '-Capsule' }
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'capture_motion_probe.ps1') @arguments
    if ($LASTEXITCODE -ne 0) { throw 'Motion capture failed.' }
    $capture = Join-Path $root "target/phase0-motion-review-$renderer-edge-$theme-$shape/folded.png"
    $destination = Join-Path $root "target/desktop-$renderer-$theme-$shape.png"
    $expandedDestination = Join-Path $root "target/desktop-$renderer-$theme-$shape-expanded.png"
    Copy-Item -LiteralPath $capture -Destination $destination -Force
    Copy-Item -LiteralPath (Join-Path (Split-Path $capture) 'open-600ms.png') -Destination $expandedDestination -Force

    function Assert-DesktopComposition([string]$path, [string]$state, [double]$surfaceWidth, [double]$surfaceHeight, [double]$bodyTop) {
        $bitmap = [Drawing.Bitmap]::FromFile($path)
        $surface = if ($Light) { 245 } else { 0 }
        $scale = $bitmap.Width / 760.0
        $halfWidth = $surfaceWidth * $scale / 2.0 + 2
        $bottom = ($bodyTop + $surfaceHeight) * $scale + 2
        $background = $backdrop.BackColor.ToArgb()
        try {
            $unexpected = 0
            for ($y = 0; $y -lt $bitmap.Height; $y++) {
                for ($x = 0; $x -lt $bitmap.Width; $x++) {
                    if ($y -lt $bottom -and [Math]::Abs($x - $bitmap.Width / 2.0) -lt $halfWidth) { continue }
                    if ($bitmap.GetPixel($x,$y).ToArgb() -ne $background) { $unexpected++ }
                }
            }
            $bodyY = [int](($bodyTop + 3) * $scale)
            $body = $bitmap.GetPixel([int]($bitmap.Width / 2), $bodyY)
            if ($unexpected -gt 0) { throw "$unexpected desktop pixels outside the $state island bounds changed: possible host rectangle, frame, or shadow." }
            if ([Math]::Abs($body.R - $surface) -gt 2 -or [Math]::Abs($body.G - $surface) -gt 2 -or [Math]::Abs($body.B - $surface) -gt 2) {
                throw "The $state island body was not visibly composited: $body"
            }

            if ($state -eq 'folded') {
                $cornerX = $(if ($Capsule) { 100 } else { 112 }) * $scale
                $cornerY = $(if ($Capsule) { 9 } else { 12 }) * $scale
                foreach ($side in @(-1,1)) {
                    $pixel = $bitmap.GetPixel([int]($bitmap.Width / 2 + $side * $cornerX), [int]$cornerY)
                    if ($pixel.ToArgb() -ne $background) { throw "The $side folded transparent corner covers the desktop: $pixel" }
                }
            }
            Write-Output "PASS $state desktop composition: visible body; unchanged background outside island bounds."
        } finally {
            $bitmap.Dispose()
        }
    }

    $foldedWidth = if ($Capsule) { 202 } else { 230 }
    $foldedHeight = if ($Capsule) { 30 } else { 30 }
    $bodyTop = if ($Capsule) { 8 } else { 0 }
    $expandedWidth = if ($Capsule) { 680 } else { 708 }
    $expandedHeight = 336
    Assert-DesktopComposition $destination 'folded' $foldedWidth $foldedHeight $bodyTop
    Assert-DesktopComposition $expandedDestination 'expanded' $expandedWidth $expandedHeight $bodyTop
    Write-Output "Desktop composition captures: $destination; $expandedDestination"
} finally {
    $backdrop.Close()
    $backdrop.Dispose()
}
