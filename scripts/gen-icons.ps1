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
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
    $g.Clear([System.Drawing.Color]::Transparent)

    $scale = [float]$size / 256.0
    function P([float]$value) { return [float]($value * $scale) }

    # Raised, forked tail. The silhouette borrows the gesture of a breaching
    # whale while the body remains the product's rounded-rectangle motif.
    $tail = New-Object System.Drawing.Drawing2D.GraphicsPath
    $tail.StartFigure()
    $tail.AddBezier((P 183), (P 77), (P 204), (P 72), (P 211), (P 51), (P 204), (P 29))
    $tail.AddBezier((P 204), (P 29), (P 225), (P 35), (P 236), (P 50), (P 235), (P 67))
    $tail.AddBezier((P 235), (P 67), (P 243), (P 59), (P 249), (P 58), (P 253), (P 62))
    $tail.AddBezier((P 253), (P 62), (P 248), (P 84), (P 232), (P 99), (P 207), (P 104))
    $tail.AddBezier((P 207), (P 104), (P 197), (P 106), (P 190), (P 102), (P 183), (P 98))
    $tail.CloseFigure()
    $outlineWidth = [float][Math]::Max(1.0, (P 8))
    $tailBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 250, 250, 250))
    $tailPen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(255, 24, 24, 24), $outlineWidth)
    $tailPen.LineJoin = [System.Drawing.Drawing2D.LineJoin]::Round
    $g.FillPath($tailBrush, $tail)
    $g.DrawPath($tailPen, $tail)

    # Continuous-corner, near-square whale body.
    $body = New-Object System.Drawing.Drawing2D.GraphicsPath
    $body.StartFigure()
    $body.AddLine((P 64), (P 20), (P 160), (P 20))
    $body.AddBezier((P 160), (P 20), (P 195), (P 20), (P 212), (P 37), (P 212), (P 72))
    $body.AddLine((P 212), (P 72), (P 212), (P 184))
    $body.AddBezier((P 212), (P 184), (P 212), (P 219), (P 195), (P 236), (P 160), (P 236))
    $body.AddLine((P 160), (P 236), (P 64), (P 236))
    $body.AddBezier((P 64), (P 236), (P 29), (P 236), (P 12), (P 219), (P 12), (P 184))
    $body.AddLine((P 12), (P 184), (P 12), (P 72))
    $body.AddBezier((P 12), (P 72), (P 12), (P 37), (P 29), (P 20), (P 64), (P 20))
    $body.CloseFigure()
    $bodyBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 250, 250, 250))
    $bodyPen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(255, 24, 24, 24), $outlineWidth)
    $bodyPen.LineJoin = [System.Drawing.Drawing2D.LineJoin]::Round
    $g.FillPath($bodyBrush, $body)
    $g.DrawPath($bodyPen, $body)

    # A single flipper gives the otherwise geometric body a whale silhouette.
    $flipper = New-Object System.Drawing.Drawing2D.GraphicsPath
    $flipper.StartFigure()
    $flipper.AddBezier((P 91), (P 148), (P 121), (P 151), (P 145), (P 171), (P 153), (P 199))
    $flipper.AddBezier((P 153), (P 199), (P 123), (P 199), (P 95), (P 181), (P 81), (P 154))
    $flipper.CloseFigure()
    $flipperBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 85, 85, 85))
    $g.FillPath($flipperBrush, $flipper)

    # Terminal-prompt eye: the solid chevron stays legible at small sizes and
    # ties the whale to dsh's command-line identity.
    $eye = New-Object System.Drawing.Drawing2D.GraphicsPath
    $eye.StartFigure()
    $eye.AddLine((P 40), (P 60), (P 56), (P 60))
    $eye.AddLine((P 56), (P 60), (P 88), (P 84))
    $eye.AddLine((P 88), (P 84), (P 56), (P 108))
    $eye.AddLine((P 56), (P 108), (P 40), (P 108))
    $eye.AddLine((P 40), (P 108), (P 71), (P 84))
    $eye.CloseFigure()
    $eyeBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 17, 17, 17))
    $g.FillPath($eyeBrush, $eye)

    $eyeBrush.Dispose(); $eye.Dispose()
    $flipperBrush.Dispose(); $flipper.Dispose()
    $bodyPen.Dispose(); $bodyBrush.Dispose(); $body.Dispose()
    $tailPen.Dispose(); $tailBrush.Dispose(); $tail.Dispose(); $g.Dispose()
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

# Tauri 2.6.x decodes only the first ICO entry for its runtime window icon.
# Windows 11's taskbar uses 24px at 100% DPI, so keep 24 first; the remaining
# entries are still available to the executable resource and Windows shell.
$sizes = @(24, 16, 20, 32, 40, 48, 64, 80, 96, 128, 256)
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
