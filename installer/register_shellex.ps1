param(
    [ValidateSet("Install", "Uninstall")]
    [string]$Action = "Install",
    [string]$InstallDir = $PSScriptRoot
)

$ErrorActionPreference = "Continue"
$logDir = Join-Path $env:LOCALAPPDATA "AnyCompare"
New-Item -ItemType Directory -Path $logDir -Force | Out-Null
$logFile = Join-Path $logDir "shellex_install.log"

function Write-Log([string]$msg) {
    $ts = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    Add-Content -Path $logFile -Value "[$ts] $msg" -Encoding UTF8
}

$dllPath = Join-Path $InstallDir "AnyCompareShellEx64.dll"
$msixPath = Join-Path $InstallDir "AnyCompareShellEx.msix"
$looseManifest = Join-Path $InstallDir "shellex\AppxManifest.xml"
$packageName = "AnyCompare.ShellExt"

if ($Action -eq "Install") {
    Write-Log "Iniciando registro do menu de contexto do AnyCompare em '$InstallDir'."

    # 1. Menu clássico do Windows 10 e Windows 11 ("Mostrar mais opções"):
    #    funciona imediatamente por usuário (HKCU), sem exigir elevação nem certificado.
    if (Test-Path $dllPath) {
        $proc = Start-Process -FilePath "regsvr32.exe" -ArgumentList @("/s", "`"$dllPath`"") -Wait -PassThru -WindowStyle Hidden
        Write-Log "regsvr32 Install exit=$($proc.ExitCode)"
    } else {
        Write-Log "AVISO: DLL não encontrada em '$dllPath'."
    }

    # 2. Menu moderno de 1º nível do Windows 11 (fora de "Mostrar mais opções"):
    #    tenta primeiro o pacote MSIX assinado; se não assinado, tenta registro
    #    desempacotado quando o Modo de Desenvolvedor do Windows está ativo.
    $win11Ok = $false
    try {
        $existing = Get-AppxPackage -Name $packageName -ErrorAction SilentlyContinue
        if ($existing) {
            $existing | Remove-AppxPackage -ErrorAction SilentlyContinue
        }
    } catch {
        Write-Log "Limpeza prévia de AppxPackage ignorada: $($_.Exception.Message)"
    }

    if (Test-Path $msixPath) {
        try {
            $sig = Get-AuthenticodeSignature -FilePath $msixPath -ErrorAction SilentlyContinue
            if ($sig -and $sig.Status -eq "Valid") {
                Add-AppxPackage -Path $msixPath -ErrorAction Stop
                Write-Log "MSIX assinado instalado com sucesso (menu de 1o nível Windows 11 ativo)."
                $win11Ok = $true
            } else {
                $statusText = if ($sig) { $sig.Status } else { "Sem assinatura" }
                Write-Log "MSIX não assinado/não confiável ($statusText); tentando registro em Modo de Desenvolvedor."
            }
        } catch {
            Write-Log "Falha ao instalar MSIX: $($_.Exception.Message)"
        }
    }

    if (-not $win11Ok -and (Test-Path $looseManifest)) {
        try {
            $devMode = (Get-ItemProperty -Path "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock" -Name "AllowDevelopmentWithoutDevLicense" -ErrorAction SilentlyContinue).AllowDevelopmentWithoutDevLicense
            if ($devMode -eq 1) {
                Add-AppxPackage -Register $looseManifest -ErrorAction Stop
                Write-Log "Menu de 1o nível Windows 11 registrado via AppxManifest (Modo de Desenvolvedor)."
                $win11Ok = $true
            } else {
                Write-Log "Modo de Desenvolvedor desativado e MSIX sem certificado confiável: menu clássico ativo. Para ativar o 1o nível no Windows 11, ative o Modo de Desenvolvedor ou assine o MSIX com um certificado confiável."
            }
        } catch {
            Write-Log "Registro via AppxManifest ignorado: $($_.Exception.Message)"
        }
    }

    Write-Log "Registro concluído (classic=OK, win11_top_level=$win11Ok)."
} else {
    Write-Log "Iniciando remoção do menu de contexto do AnyCompare."
    try {
        $existing = Get-AppxPackage -Name $packageName -ErrorAction SilentlyContinue
        if ($existing) {
            $existing | Remove-AppxPackage -ErrorAction SilentlyContinue
            Write-Log "Pacote MSIX '$packageName' removido."
        }
    } catch {
        Write-Log "Erro ao remover AppxPackage: $($_.Exception.Message)"
    }

    if (Test-Path $dllPath) {
        $proc = Start-Process -FilePath "regsvr32.exe" -ArgumentList @("/u", "/s", "`"$dllPath`"") -Wait -PassThru -WindowStyle Hidden
        Write-Log "regsvr32 Uninstall exit=$($proc.ExitCode)"
    }
    Write-Log "Remoção concluída."
}

exit 0
