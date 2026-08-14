# Generates src-tauri/icons: a multi-size icon.ico (PNG-compressed entries,
# fine on Windows 7+) and icon.png at 256px. Windows-only (uses System.Drawing).
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing

$root = Split-Path -Parent $PSScriptRoot
$outDir = Join-Path $root "src-tauri\icons"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null

function New-IconBitmap([int]$size) {
    $bmp = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit
    $g.Clear([System.Drawing.Color]::Transparent)

    # Rounded dark-blue tile.
    $d = [float]($size * 0.22)
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $path.AddArc(0, 0, $d, $d, 180, 90)
    $path.AddArc($size - $d, 0, $d, $d, 270, 90)
    $path.AddArc($size - $d, $size - $d, $d, $d, 0, 90)
    $path.AddArc(0, $size - $d, $d, $d, 90, 90)
    $path.CloseFigure()
    $rect = New-Object System.Drawing.RectangleF(0, 0, $size, $size)
    $brush = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
        $rect,
        [System.Drawing.Color]::FromArgb(255, 46, 78, 138),
        [System.Drawing.Color]::FromArgb(255, 12, 20, 38),
        90)
    $g.FillPath($brush, $path)

    # White "dsh" wordmark.
    $font = New-Object System.Drawing.Font("Segoe UI", [float]($size * 0.32),
        [System.Drawing.FontStyle]::Bold, [System.Drawing.GraphicsUnit]::Pixel)
    $format = New-Object System.Drawing.StringFormat
    $format.Alignment = [System.Drawing.StringAlignment]::Center
    $format.LineAlignment = [System.Drawing.StringAlignment]::Center
    $g.DrawString("dsh", $font, [System.Drawing.Brushes]::White, $rect, $format)

    $font.Dispose(); $format.Dispose(); $brush.Dispose(); $path.Dispose(); $g.Dispose()
    return $bmp
}

function Save-Png([System.Drawing.Bitmap]$bmp, [string]$path) {
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
}

function ConvertTo-Ico([byte[][]]$pngs, [int[]]$sizes) {
    $count = $pngs.Count
    $headerSize = 6 + 16 * $count
    $offset = $headerSize
    $ms = New-Object System.IO.MemoryStream
    $bw = New-Object System.IO.BinaryWriter($ms)
    $bw.Write([uint16]0); $bw.Write([uint16]1); $bw.Write([uint16]$count)
    for ($i = 0; $i -lt $count; $i++) {
        $w = if ($sizes[$i] -ge 256) { 0 } else { $sizes[$i] }
        $bw.Write([byte]$w); $bw.Write([byte]$w)
        $bw.Write([byte]0); $bw.Write([byte]0)
        $bw.Write([uint16]1); $bw.Write([uint16]32)
        $bw.Write([uint32]$pngs[$i].Length); $bw.Write([uint32]$offset)
        $offset += $pngs[$i].Length
    }
    for ($i = 0; $i -lt $count; $i++) { $bw.Write($pngs[$i]) }
    $bw.Flush()
    return $ms.ToArray()
}

$sizes = @(16, 24, 32, 48, 64, 128, 256)
$pngs = New-Object 'System.Collections.Generic.List[byte[]]'
foreach ($size in $sizes) {
    $bmp = New-IconBitmap $size
    $pngPath = Join-Path $env:TEMP ("dsh-icon-{0}.png" -f $size)
    Save-Png $bmp $pngPath
    if ($size -eq 256) { Copy-Item $pngPath (Join-Path $outDir "icon.png") -Force }
    $pngs.Add([System.IO.File]::ReadAllBytes($pngPath))
    $bmp.Dispose()
    Remove-Item $pngPath -Force
}
[System.IO.File]::WriteAllBytes((Join-Path $outDir "icon.ico"), (ConvertTo-Ico $pngs.ToArray() $sizes))
Write-Output ("wrote " + (Join-Path $outDir "icon.ico") + " and " + (Join-Path $outDir "icon.png"))
