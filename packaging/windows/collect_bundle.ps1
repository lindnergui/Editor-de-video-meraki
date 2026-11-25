param(
    [string]$BinaryPath = "target\release\meraki-app.exe",
    [string]$GStreamerRoot = $env:GSTREAMER_1_0_ROOT_MSVC_X86_64,
    [string]$OutputDir = "dist\windows\meraki"
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($GStreamerRoot)) {
    $machineVar = [Environment]::GetEnvironmentVariable("GSTREAMER_1_0_ROOT_MSVC_X86_64", "Machine")
    if (-not [string]::IsNullOrWhiteSpace($machineVar)) {
        $GStreamerRoot = $machineVar
    } else {
        $GStreamerRoot = "C:\gstreamer\1.0\msvc_x86_64"
    }
}

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
if (-not [System.IO.Path]::IsPathRooted($BinaryPath)) {
    $BinaryPath = Join-Path $RepoRoot $BinaryPath
}
if (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $RepoRoot $OutputDir
}

if (-not (Test-Path $BinaryPath -PathType Leaf)) {
    throw "Executável release não encontrado em: $BinaryPath (rode 'cargo build --release --bin meraki-app')"
}
if (-not (Test-Path $GStreamerRoot -PathType Container)) {
    throw "Diretório raiz do GStreamer MSVC x86_64 não encontrado em: $GStreamerRoot"
}

$GstBinDir = Join-Path $GStreamerRoot "bin"
$GstPluginSrc = Join-Path $GStreamerRoot "lib\gstreamer-1.0"
if (-not (Test-Path $GstBinDir -PathType Container)) {
    throw "Pasta bin do GStreamer não encontrada em: $GstBinDir"
}
if (-not (Test-Path $GstPluginSrc -PathType Container)) {
    throw "Pasta de plugins lib\gstreamer-1.0 não encontrada em: $GstPluginSrc"
}

if (Test-Path $OutputDir) {
    Remove-Item -Path $OutputDir -Recurse -Force
}

$PluginOutDir = Join-Path $OutputDir "lib\gstreamer-1.0"
$LibexecOutDir = Join-Path $OutputDir "libexec\gstreamer-1.0"
New-Item -ItemType Directory -Path $OutputDir, $PluginOutDir, $LibexecOutDir -Force | Out-Null

# 1. Executável principal renomeado para meraki.exe
Copy-Item -Path $BinaryPath -Destination (Join-Path $OutputDir "meraki.exe") -Force

# 2. DLLs de runtime do GStreamer / GLib / FFmpeg / codecs em bin\*.dll
$RuntimeDlls = Get-ChildItem -Path $GstBinDir -Filter "*.dll" -File
foreach ($dll in $RuntimeDlls) {
    Copy-Item -Path $dll.FullName -Destination $OutputDir -Force
}

# 3. Plugins nativos do GStreamer em lib\gstreamer-1.0\*.dll
$PluginDlls = Get-ChildItem -Path $GstPluginSrc -Filter "*.dll" -File
foreach ($plugin in $PluginDlls) {
    Copy-Item -Path $plugin.FullName -Destination $PluginOutDir -Force
}

# 4. gst-plugin-scanner.exe para descoberta isolada de plugins sem congelar a inicialização
$ScannerCandidates = @(
    (Join-Path $GStreamerRoot "libexec\gstreamer-1.0\gst-plugin-scanner.exe"),
    (Join-Path $GstBinDir "gst-plugin-scanner.exe")
)
$ScannerExe = $ScannerCandidates | Where-Object { Test-Path $_ -PathType Leaf } | Select-Object -First 1
if (-not $ScannerExe) {
    throw "gst-plugin-scanner.exe não encontrado no runtime GStreamer ($GStreamerRoot)"
}
Copy-Item -Path $ScannerExe -Destination (Join-Path $LibexecOutDir "gst-plugin-scanner.exe") -Force

# 5. Configuração opcional do Fontconfig para textos e títulos GES
$EtcFonts = Join-Path $GStreamerRoot "etc\fonts"
if (Test-Path $EtcFonts -PathType Container) {
    Copy-Item -Path $EtcFonts -Destination (Join-Path $OutputDir "etc\fonts") -Recurse -Force
}

# 6. Licenças open-source no diretório distribuído
foreach ($license in @("LICENSE-MIT", "LICENSE-APACHE")) {
    $src = Join-Path $RepoRoot $license
    if (Test-Path $src -PathType Leaf) {
        Copy-Item -Path $src -Destination $OutputDir -Force
    }
}

# Validação final dos arquivos críticos
foreach ($required in @("meraki.exe", "gstreamer-1.0-0.dll", "ges-1.0-0.dll")) {
    $target = Join-Path $OutputDir $required
    if (-not (Test-Path $target -PathType Leaf)) {
        throw "Arquivo obrigatório ausente no bundle: $target"
    }
}

$CopiedPlugins = (Get-ChildItem -Path $PluginOutDir -Filter "*.dll" -File).Count
if ($CopiedPlugins -lt 1) {
    throw "Nenhum plugin GStreamer (.dll) copiado para $PluginOutDir"
}

Write-Host "Bundle Windows gerado em: $OutputDir"
Write-Host ("Runtime DLLs: {0} | Plugins: {1}" -f $RuntimeDlls.Count, $CopiedPlugins)
