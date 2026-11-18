param(
    [string]$Name = "soundy",
    [int]$Seconds = 45,
    [int]$Settle = 15,
    [string]$Exe,
    [string]$Output = ""
)

$ErrorActionPreference = "Stop"

if ($Exe) {
    Start-Process -FilePath (Resolve-Path $Exe).Path | Out-Null
}

if ($Settle -gt 0) {
    Start-Sleep -Seconds $Settle
}

$targets = @(Get-Process -Name $Name -ErrorAction SilentlyContinue)
if ($targets.Count -eq 0) {
    throw "No process named '$Name' is running"
}

$cores = [Environment]::ProcessorCount
$samples = New-Object System.Collections.Generic.List[object]
$lastCpu = @{}
$lastAt = [DateTime]::UtcNow

for ($i = 0; $i -lt $Seconds; $i++) {
    Start-Sleep -Seconds 1
    $targets = @(Get-Process -Name $Name -ErrorAction SilentlyContinue)
    $now = [DateTime]::UtcNow
    $elapsed = ($now - $lastAt).TotalSeconds
    $cpuDelta = 0.0
    foreach ($process in $targets) {
        $process.Refresh()
        if ($lastCpu.ContainsKey($process.Id)) {
            $cpuDelta += ($process.TotalProcessorTime - $lastCpu[$process.Id]).TotalSeconds
        }
        $lastCpu[$process.Id] = $process.TotalProcessorTime
    }
    $percent = if ($elapsed -gt 0) {
        ($cpuDelta / $elapsed) / $cores * 100.0
    } else {
        0.0
    }
    $rss = ($targets | Measure-Object WorkingSet64 -Sum).Sum
    $private = ($targets | Measure-Object PrivateMemorySize64 -Sum).Sum
    $samples.Add([pscustomobject]@{
        At = $now.ToString("o")
        Processes = $targets.Count
        WorkingSetMB = [math]::Round($rss / 1MB, 2)
        PrivateMB = [math]::Round($private / 1MB, 2)
        CpuPercent = [math]::Round($percent, 3)
    })
    $lastAt = $now
}

function Get-Median($values) {
    $ordered = @($values | Sort-Object)
    if ($ordered.Count -eq 0) { return 0 }
    $middle = [int][math]::Floor($ordered.Count / 2)
    if ($ordered.Count % 2 -eq 1) { return $ordered[$middle] }
    return ($ordered[$middle - 1] + $ordered[$middle]) / 2
}

$idleRss = Get-Median ($samples | ForEach-Object { $_.WorkingSetMB })
$idlePrivate = Get-Median ($samples | ForEach-Object { $_.PrivateMB })
$idleCpu = Get-Median ($samples | ForEach-Object { $_.CpuPercent })
$peakRss = ($samples | Measure-Object WorkingSetMB -Maximum).Maximum

$summary = [pscustomobject]@{
    Process = $Name
    Samples = $samples.Count
    SettleSeconds = $Settle
    MedianRssMB = [math]::Round($idleRss, 2)
    PeakRssMB = [math]::Round($peakRss, 2)
    MedianPrivateMB = [math]::Round($idlePrivate, 2)
    MedianCpuPercent = [math]::Round($idleCpu, 3)
}

$summary | Format-List

if ($Output) {
    $samples | Export-Csv -Path $Output -NoTypeInformation
    $summary | ConvertTo-Json | Set-Content -Path ($Output -replace "\.csv$", ".summary.json")
    Write-Output "samples written to $Output"
}
