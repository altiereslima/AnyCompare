<#
.SYNOPSIS
    Compila o AnyCompare + extensão de menu de contexto Windows 11 e gera automaticamente
    o instalador Inno Setup em dist\AnyCompare-Setup-<versão>.exe.
.EXAMPLE
    .\build.ps1
    .\build.ps1 -WithTests
    .\build.ps1 -CertPfx .\installer\certs\AnyCompare.pfx -CertPassword "senha"
#>
param(
    [switch]$WithTests,
    [switch]$SkipInstaller,
    [switch]$BumpPatch,
    [string]$CertPfx = "",
    [string]$CertPassword = ""
)

& (Join-Path $PSScriptRoot "tools\build_installer.ps1") `
    -WithTests:$WithTests `
    -SkipInstaller:$SkipInstaller `
    -BumpPatch:$BumpPatch `
    -CertPfx $CertPfx `
    -CertPassword $CertPassword
exit $LASTEXITCODE
