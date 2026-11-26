param(
    [string]$NativeBuild,
    [string]$RuntimeAssets,
    [string]$SuperMotionEngine,
    [string]$Destination
)
$ErrorActionPreference = 'Stop'
$project = (Resolve-Path $PSScriptRoot).Path
if (!$NativeBuild -or !$RuntimeAssets) {
    throw 'Pass -NativeBuild (portal_media.dll and portal_media.lib) and -RuntimeAssets (media DLLs, shaders and models).'
}
if (!$Destination) { $Destination = Join-Path $project 'dist' }
$env:PORTAL_NATIVE_DIR = (Resolve-Path $NativeBuild).Path
cargo build --locked --manifest-path (Join-Path $project 'Cargo.toml')
if ($LASTEXITCODE) { throw 'Cargo build failed' }
$experimental = Join-Path $Destination 'runtime/experimental'
$licenses = Join-Path $Destination 'licenses'
New-Item -ItemType Directory -Force $experimental,$licenses,(Join-Path $Destination 'runtime_local') | Out-Null
Copy-Item -LiteralPath (Join-Path $project 'target/debug/veloranr.exe'),(Join-Path $NativeBuild 'portal_media.dll') -Destination $Destination -Force
foreach ($name in 'avcodec-63.dll','avformat-63.dll','avutil-61.dll','avfilter-12.dll','swresample-7.dll','swscale-10.dll','dav1d.dll','opus.dll','libcrypto-3-x64.dll','libssl-3-x64.dll','D3Dcompiler_47.dll','dolby_vision.dll','dovi.dll','msvcp140.dll','vcruntime140.dll','vcruntime140_1.dll') {
    $file = Join-Path $RuntimeAssets $name
    if (Test-Path -LiteralPath $file) { Copy-Item -LiteralPath $file -Destination $Destination -Force }
}
foreach ($name in 'shaders','runtime/models','runtime/config','runtime/experimental/nr-ampere') {
    $dir = Join-Path $Destination $name
    New-Item -ItemType Directory -Path $dir -Force | Out-Null
    Get-ChildItem -LiteralPath (Join-Path $RuntimeAssets $name) | Copy-Item -Destination $dir -Recurse -Force
}
foreach ($name in 'nvngx_vsr.dll','nvngx_dlss.dll','nvngx_truehdr.dll') {
    Copy-Item -LiteralPath (Join-Path $RuntimeAssets "runtime/experimental/$name") -Destination $experimental -Force
}
Copy-Item -LiteralPath (Join-Path $RuntimeAssets 'LICENSE'),(Join-Path $RuntimeAssets 'THIRD_PARTY_NOTICES.md') -Destination $Destination -Force
Get-ChildItem -LiteralPath (Join-Path $RuntimeAssets 'licenses') | Copy-Item -Destination $licenses -Recurse -Force
Copy-Item -LiteralPath (Join-Path $project 'prototypes/fonts/OFL-Instrument-Sans.txt') -Destination $licenses -Force

# Keep each supplied frame-generation runtime in the directory its loader uses.
foreach ($name in 'runtime/nvidia-vfg','runtime/experimental/dlssg-kernels','runtime/experimental/dlssg-lite','runtime/experimental/fallback-dlssg-310.7','runtime_local/intel','runtime_local/amd') {
    $source = Join-Path $RuntimeAssets $name
    if (Test-Path -LiteralPath $source -PathType Container) {
        $target = Join-Path $Destination $name
        New-Item -ItemType Directory -Force $target | Out-Null
        Get-ChildItem -LiteralPath $source | Copy-Item -Destination $target -Recurse -Force
    }
}
# Stage only the SMFG core. NVIDIA and Intel sidecar packs are separate products.
if ($SuperMotionEngine) {
    $source = (Resolve-Path -LiteralPath $SuperMotionEngine).Path
    $target = Join-Path $Destination 'framegen/engine'
    New-Item -ItemType Directory -Force $target | Out-Null
    Copy-Item -LiteralPath (Join-Path $source 'nvngx.dll') -Destination $target -Force
    foreach ($pattern in 'msvcp*.dll','concrt*.dll','vcruntime*.dll') {
        Get-ChildItem -LiteralPath $source -Filter $pattern -File | Copy-Item -Destination $target -Force
    }
    Copy-Item -LiteralPath (Join-Path $source 'ai') -Destination $target -Recurse -Force
}
$dlssg = Join-Path $RuntimeAssets 'runtime/experimental/nvngx_dlssg.dll'
if (Test-Path -LiteralPath $dlssg -PathType Leaf) {
    Copy-Item -LiteralPath $dlssg -Destination $experimental -Force
}
