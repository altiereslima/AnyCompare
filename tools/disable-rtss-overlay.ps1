[CmdletBinding(SupportsShouldProcess)]
param(
    [string[]] $Executable,
    [switch] $Check
)

$ErrorActionPreference = 'Stop'

if (-not $Executable) {
    $dist = Join-Path (Split-Path -Parent $PSScriptRoot) 'dist'
    $latest = Get-ChildItem -LiteralPath (Join-Path $dist 'preview') -Filter 'AnyCompare-*.exe' -File -ErrorAction SilentlyContinue |
        Where-Object Name -Match '^AnyCompare-\d+\.\d+\.\d+\.exe$' |
        Sort-Object { [version] ($_.BaseName -replace '^AnyCompare-', '') } -Descending |
        Select-Object -First 1
    $Executable = @((Join-Path $dist 'AnyCompare.exe'), $latest.FullName) | Where-Object { $_ -and (Test-Path -LiteralPath $_ -PathType Leaf) }
}

$names = @($Executable | ForEach-Object {
    $file = Get-Item -LiteralPath $_
    if ($file.PSIsContainer -or $file.Name -notmatch '^AnyCompare(?:-\d+\.\d+\.\d+)?\.exe$') {
        throw "Escolha um executável do AnyCompare. Recebido: $_"
    }
    $file.Name
} | Sort-Object -Unique)
if (!$names.Count) { throw 'Nenhum executável do AnyCompare encontrado. Informe -Executable.' }

$registry = [Microsoft.Win32.RegistryKey]::OpenBaseKey(
    [Microsoft.Win32.RegistryHive]::LocalMachine, [Microsoft.Win32.RegistryView]::Registry32)
try {
    $rtss = $registry.OpenSubKey('SOFTWARE\Unwinder\RTSS')
    if (!$rtss) { throw 'Instalação do RivaTuner Statistics Server não encontrada.' }
    try {
        $installPath = $rtss.GetValue('InstallPath')
    } finally {
        $rtss.Dispose()
    }
} finally {
    $registry.Dispose()
}
if (!$installPath) { throw 'A instalação do RTSS não informa o caminho do executável.' }
$library = if ([Environment]::Is64BitProcess) { 'RTSSHooks64.dll' } else { 'RTSSHooks.dll' }
$dll = Get-Item -LiteralPath (Join-Path (Split-Path -Parent $installPath) $library)

if (-not $Check) {
    $names = @($names | Where-Object { $PSCmdlet.ShouldProcess($_, 'Desativar detecção no perfil do RTSS') })
    if (!$names.Count) { return }
}

# Match the C ABI from RTSSProfileInterface.h in the installed RTSS SDK.
$dllPath = $dll.FullName.Replace('"', '""')
if (-not ('AnyCompare.RtssProfiles' -as [type])) {
    Add-Type -TypeDefinition @"
using System.Runtime.InteropServices;
using System.Text;

namespace AnyCompare {
    public static class RtssProfiles {
        [DllImport(@"$dllPath", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
        public static extern uint EnumProfiles(StringBuilder names, uint size);
        [DllImport(@"$dllPath", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
        public static extern void LoadProfile(string name);
        [DllImport(@"$dllPath", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
        public static extern void SaveProfile(string name);
        [DllImport(@"$dllPath", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
        public static extern bool GetProfileProperty(string property, out uint value, uint size);
        [DllImport(@"$dllPath", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
        public static extern bool SetProfileProperty(string property, ref uint value, uint size);
        [DllImport(@"$dllPath", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
        public static extern void UpdateProfiles();
    }
}
"@
}

function Get-RtssProfileNames {
    $buffer = [System.Text.StringBuilder]::new(4096)
    $size = [AnyCompare.RtssProfiles]::EnumProfiles($buffer, $buffer.Capacity)
    if ($size -gt $buffer.Capacity) {
        $buffer = [System.Text.StringBuilder]::new([int]$size)
        if ([AnyCompare.RtssProfiles]::EnumProfiles($buffer, $buffer.Capacity) -gt $buffer.Capacity) {
            throw 'Os perfis do RTSS mudaram durante a leitura. Tente novamente.'
        }
    }
    $buffer.ToString().Split(',') | ForEach-Object { $_.Trim() }
}

foreach ($name in $names) {
    $hasProfile = (Get-RtssProfileNames) -contains $name
    [AnyCompare.RtssProfiles]::LoadProfile($name)
    [uint32] $level = 0
    if (-not [AnyCompare.RtssProfiles]::GetProfileProperty('AppDetectionLevel', [ref]$level, 4)) {
        throw "O RTSS não retornou o nível de detecção de $name."
    }

    if (!$Check -and (!$hasProfile -or $level -ne 0)) {
        $level = 0
        if (-not [AnyCompare.RtssProfiles]::SetProfileProperty('AppDetectionLevel', [ref]$level, 4)) {
            throw "O RTSS recusou a configuração para $name."
        }
        [AnyCompare.RtssProfiles]::SaveProfile($name)
        [AnyCompare.RtssProfiles]::LoadProfile($name)
        $hasProfile = (Get-RtssProfileNames) -contains $name
        if (!$hasProfile -or
            ![AnyCompare.RtssProfiles]::GetProfileProperty('AppDetectionLevel', [ref]$level, 4) -or $level -ne 0) {
            throw "O RTSS não gravou o perfil de $name. Execute o script no PowerShell como administrador."
        }
        [AnyCompare.RtssProfiles]::UpdateProfiles()
    }

    [pscustomobject]@{ Executavel = $name; PerfilProprio = $hasProfile; NivelDeDeteccao = $level }
}
