# Generates beanxsound.ico (7 sizes) and icon.png in the black / liquid chrome /
# hot pink Y2K style used by the app's default "Y2K Chrome" theme.
param([string]$OutDir = (Join-Path $PSScriptRoot '..\assets'))

Add-Type -AssemblyName System.Drawing

$OutDir = [System.IO.Path]::GetFullPath($OutDir)
if (-not (Test-Path $OutDir)) { New-Item -ItemType Directory -Path $OutDir | Out-Null }

function C([int]$r, [int]$g, [int]$b, [int]$a = 255) {
    return [System.Drawing.Color]::FromArgb($a, $r, $g, $b)
}

function New-RoundedPath([single]$x, [single]$y, [single]$w, [single]$h, [single]$r) {
    $path = [System.Drawing.Drawing2D.GraphicsPath]::new()
    $d = $r * 2.0
    $path.AddArc($x, $y, $d, $d, 180, 90)
    $path.AddArc(($x + $w - $d), $y, $d, $d, 270, 90)
    $path.AddArc(($x + $w - $d), ($y + $h - $d), $d, $d, 0, 90)
    $path.AddArc($x, ($y + $h - $d), $d, $d, 90, 90)
    $path.CloseFigure()
    return $path
}

function New-LinearBrush([System.Drawing.RectangleF]$rect, $from, $to, [single]$angle) {
    $b = [System.Drawing.Drawing2D.LinearGradientBrush]::new($rect, $from, $to, $angle)
    $b.WrapMode = [System.Drawing.Drawing2D.WrapMode]::TileFlipXY
    return $b
}

$S = [single]1024.0
$bmp = [System.Drawing.Bitmap]::new([int]$S, [int]$S, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
$g.Clear([System.Drawing.Color]::Transparent)

$margin = [single]26.0
$tileW = $S - (2.0 * $margin)
$tile = [System.Drawing.RectangleF]::new($margin, $margin, $tileW, $tileW)
$radius = [single]232.0
$tilePath = New-RoundedPath $tile.X $tile.Y $tile.Width $tile.Height $radius

# --- soft pink halo behind the tile
for ($i = 7; $i -ge 1; $i--) {
    $pad = [single]($i * 9.0)
    $haloRect = [System.Drawing.RectangleF]::new(($tile.X - $pad), ($tile.Y - $pad), ($tile.Width + 2.0 * $pad), ($tile.Height + 2.0 * $pad))
    $halo = New-RoundedPath $haloRect.X $haloRect.Y $haloRect.Width $haloRect.Height ($radius + $pad)
    $brush = [System.Drawing.SolidBrush]::new((C 255 61 154 12))
    $g.FillPath($brush, $halo)
    $brush.Dispose(); $halo.Dispose()
}

# --- black glass tile
$tileBrush = New-LinearBrush $tile (C 30 24 36) (C 4 4 6) 90.0
$g.FillPath($tileBrush, $tilePath)
$tileBrush.Dispose()

# --- faint Y2K grid, clipped to the tile
$oldClip = $g.Clip
$g.SetClip($tilePath)
$gridPen = [System.Drawing.Pen]::new((C 244 248 255 20), 2.0)
$step = [single]116.0
$px = $tile.X
while ($px -le $tile.Right) {
    $g.DrawLine($gridPen, $px, $tile.Y, $px, $tile.Bottom)
    $px += $step
}
$py = $tile.Y
while ($py -le $tile.Bottom) {
    $g.DrawLine($gridPen, $tile.X, $py, $tile.Right, $py)
    $py += $step
}
$gridPen.Dispose()

# --- pink light pooling at the bottom
$glowRect = [System.Drawing.RectangleF]::new($tile.X, ($tile.Y + $tile.Height * 0.46), $tile.Width, ($tile.Height * 0.56))
$glow = New-LinearBrush $glowRect (C 255 61 154 95) (C 255 61 154 0) 90.0
$g.FillRectangle($glow, $glowRect)
$glow.Dispose()

# --- gloss on the top half
$glossRect = [System.Drawing.RectangleF]::new($tile.X, $tile.Y, $tile.Width, ($tile.Height * 0.52))
$gloss = New-LinearBrush $glossRect (C 255 255 255 72) (C 255 255 255 0) 90.0
$g.FillRectangle($gloss, $glossRect)
$gloss.Dispose()
$g.Clip = $oldClip

# --- chrome rim + inner pink hairline
$rimRect = [System.Drawing.RectangleF]::new(($tile.X + 4.0), ($tile.Y + 4.0), ($tile.Width - 8.0), ($tile.Height - 8.0))
$rimPath = New-RoundedPath $rimRect.X $rimRect.Y $rimRect.Width $rimRect.Height ($radius - 4.0)
$rim = New-LinearBrush $rimRect (C 255 255 255 235) (C 138 148 166 150) 90.0
$rimPen = [System.Drawing.Pen]::new($rim, 7.0)
$g.DrawPath($rimPen, $rimPath)
$rimPen.Dispose(); $rim.Dispose()

$innerRect = [System.Drawing.RectangleF]::new(($tile.X + 22.0), ($tile.Y + 22.0), ($tile.Width - 44.0), ($tile.Height - 44.0))
$innerPath = New-RoundedPath $innerRect.X $innerRect.Y $innerRect.Width $innerRect.Height ($radius - 22.0)
$innerPen = [System.Drawing.Pen]::new((C 255 61 154 120), 3.0)
$g.DrawPath($innerPen, $innerPath)
$innerPen.Dispose()

# --- five chrome-pink EQ bars
$heights = @(0.30, 0.58, 0.90, 0.54, 0.32)
$barW = [single]($S * 0.086)
$gap = [single]($S * 0.048)
$total = $barW * 5.0 + $gap * 4.0
$bx = [single]($S / 2.0 - $total / 2.0)
$maxH = [single]($S * 0.50)
$midY = [single]($S * 0.52)

foreach ($h in $heights) {
    $barH = [single]($maxH * $h)
    $barRect = [System.Drawing.RectangleF]::new($bx, ($midY - $barH / 2.0), $barW, $barH)
    $barPath = New-RoundedPath $barRect.X $barRect.Y $barRect.Width $barRect.Height ($barW / 2.0)

    # pink bloom
    $bloomRect = [System.Drawing.RectangleF]::new(($barRect.X - 16.0), ($barRect.Y - 16.0), ($barRect.Width + 32.0), ($barRect.Height + 32.0))
    $bloomPath = New-RoundedPath $bloomRect.X $bloomRect.Y $bloomRect.Width $bloomRect.Height (($barRect.Width + 32.0) / 2.0)
    $bloomBrush = [System.Drawing.SolidBrush]::new((C 255 46 136 46))
    $g.FillPath($bloomBrush, $bloomPath)
    $bloomBrush.Dispose(); $bloomPath.Dispose()

    $barBrush = New-LinearBrush $barRect (C 255 196 221) (C 255 46 136) 90.0
    $g.FillPath($barBrush, $barPath)
    $barBrush.Dispose()

    # chrome spark on the upper left of each bar
    $sparkW = [single]($barRect.Width * 0.44)
    $sparkH = [single][Math]::Max($barRect.Height * 0.34, 10.0)
    $sparkRect = [System.Drawing.RectangleF]::new(($barRect.X + $barRect.Width * 0.16), ($barRect.Y + 10.0), $sparkW, $sparkH)
    $sparkPath = New-RoundedPath $sparkRect.X $sparkRect.Y $sparkRect.Width $sparkRect.Height ($sparkW / 2.0)
    $sparkBrush = [System.Drawing.SolidBrush]::new((C 255 255 255 150))
    $g.FillPath($sparkBrush, $sparkPath)
    $sparkBrush.Dispose(); $sparkPath.Dispose(); $barPath.Dispose()

    $bx += $barW + $gap
}

$rimPath.Dispose(); $innerPath.Dispose(); $tilePath.Dispose()
$g.Dispose()

# --- downscale to every size and pack the ICO
$sizes = @(256, 128, 64, 48, 32, 24, 16)
$blobs = New-Object System.Collections.ArrayList
foreach ($sz in $sizes) {
    $small = [System.Drawing.Bitmap]::new([int]$sz, [int]$sz, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $sg = [System.Drawing.Graphics]::FromImage($small)
    $sg.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $sg.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $sg.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
    $sg.DrawImage($bmp, 0, 0, [int]$sz, [int]$sz)
    $sg.Dispose()

    $ms = [System.IO.MemoryStream]::new()
    $small.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    [void]$blobs.Add($ms.ToArray())
    if ($sz -eq 256) { $small.Save((Join-Path $OutDir 'icon.png'), [System.Drawing.Imaging.ImageFormat]::Png) }
    $small.Dispose(); $ms.Dispose()
}
$bmp.Dispose()

$icoPath = Join-Path $OutDir 'beanxsound.ico'
$fs = [System.IO.File]::Create($icoPath)
$bw = [System.IO.BinaryWriter]::new($fs)
$bw.Write([UInt16]0)
$bw.Write([UInt16]1)
$bw.Write([UInt16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $sz = $sizes[$i]
    $dim = if ($sz -ge 256) { 0 } else { $sz }
    $bw.Write([byte]$dim)
    $bw.Write([byte]$dim)
    $bw.Write([byte]0)
    $bw.Write([byte]0)
    $bw.Write([UInt16]1)
    $bw.Write([UInt16]32)
    $bw.Write([UInt32]$blobs[$i].Length)
    $bw.Write([UInt32]$offset)
    $offset += $blobs[$i].Length
}
foreach ($blob in $blobs) { $bw.Write($blob) }
$bw.Flush(); $bw.Dispose(); $fs.Dispose()

Write-Output "wrote $icoPath ($((Get-Item $icoPath).Length) bytes)"
Write-Output "wrote $(Join-Path $OutDir 'icon.png')"
