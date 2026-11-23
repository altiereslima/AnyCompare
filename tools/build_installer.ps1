param(
    [switch]$WithTests,
    [switch]$SkipInstaller,
    [switch]$BumpPatch,
    [string]$CertPfx = "",
    [string]$CertPassword = ""
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$CargoExe = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
if (-not (Test-Path $CargoExe)) { $CargoExe = "cargo" }

# 1. Ler (e opcionalmente incrementar) versão em Cargo.toml, shellex\Cargo.toml e Cargo.lock
$rootCargoPath = Join-Path $RepoRoot "Cargo.toml"
$shellexCargoPath = Join-Path $RepoRoot "shellex\Cargo.toml"
$lockPath = Join-Path $RepoRoot "Cargo.lock"
$cargoToml = Get-Content $rootCargoPath -Raw
$versionMatch = [regex]::Match($cargoToml, '(?m)^version\s*=\s*"([^"]+)"')
if (-not $versionMatch.Success) {
    throw "Não foi possível ler a versão em Cargo.toml"
}
$OldVersion = $versionMatch.Groups[1].Value
$AppVersion = $OldVersion

if ($BumpPatch) {
    if ($OldVersion -match '^(\d+)\.(\d+)\.(\d+)$') {
        $AppVersion = "$($Matches[1]).$($Matches[2]).$([int]$Matches[3] + 1)"
        Write-Host "Atualizando versão: $OldVersion -> $AppVersion" -ForegroundColor Magenta
        $utf8NoBom = [System.Text.UTF8Encoding]::new($false)
        $newRoot = [regex]::Replace($cargoToml, '(?m)^(version\s*=\s*)"[^"]+"', "`$1`"$AppVersion`"", 1)
        [System.IO.File]::WriteAllText($rootCargoPath, $newRoot, $utf8NoBom)
        if (Test-Path $shellexCargoPath) {
            $shToml = Get-Content $shellexCargoPath -Raw
            $newSh = [regex]::Replace($shToml, '(?m)^(version\s*=\s*)"[^"]+"', "`$1`"$AppVersion`"", 1)
            [System.IO.File]::WriteAllText($shellexCargoPath, $newSh, $utf8NoBom)
        }
        if (Test-Path $lockPath) {
            $lockTxt = Get-Content $lockPath -Raw
            $lockTxt = [regex]::Replace($lockTxt, '(?ms)(name\s*=\s*"anycompare"\r?\nversion\s*=\s*)"[^"]+"', "`$1`"$AppVersion`"")
            $lockTxt = [regex]::Replace($lockTxt, '(?ms)(name\s*=\s*"anycompare-shellex"\r?\nversion\s*=\s*)"[^"]+"', "`$1`"$AppVersion`"")
            [System.IO.File]::WriteAllText($lockPath, $lockTxt, $utf8NoBom)
        }
    } else {
        throw "Formato de versão inesperado para -BumpPatch: $OldVersion"
    }
}

$AppxVersion = if ($AppVersion -match '^\d+\.\d+\.\d+$') { "$AppVersion.0" } else { "0.2.6.0" }
Write-Host "=== AnyCompare Build & Installer Pipeline v$AppVersion ===" -ForegroundColor Cyan

Push-Location $RepoRoot
try {
    if ($WithTests) {
        Write-Host "[1/5] Executando clippy e testes do workspace..." -ForegroundColor Yellow
        & $CargoExe clippy -p anycompare -p anycompare-shellex --no-deps --all-targets --offline --locked -- -D warnings
        if ($LASTEXITCODE -ne 0) { throw "Clippy falhou." }
        & $CargoExe test -p anycompare -p anycompare-shellex --offline --locked
        if ($LASTEXITCODE -ne 0) { throw "Testes falharam." }
    } else {
        Write-Host "[1/5] Pulando testes (use -WithTests para executar clippy + cargo test)." -ForegroundColor DarkGray
    }

    Write-Host "[2/5] Compilando anycompare.exe + anycompare_shellex.dll (release)..." -ForegroundColor Yellow
    & $CargoExe build -p anycompare -p anycompare-shellex --release --offline --locked
    if ($LASTEXITCODE -ne 0) { throw "Cargo build --release falhou." }

    $ExePath = Join-Path $RepoRoot "target\release\anycompare.exe"
    $DllPath = Join-Path $RepoRoot "target\release\anycompare_shellex.dll"
    if (-not (Test-Path $ExePath)) { throw "Binário não encontrado: $ExePath" }
    if (-not (Test-Path $DllPath)) { throw "DLL ShellEx não encontrada: $DllPath" }

    if ($SkipInstaller) {
        Write-Host "Build concluído (instalador ignorado via -SkipInstaller)." -ForegroundColor Green
        return
    }

    # 3. Preparar diretório de staging + imagens do wizard + pacote MSIX
    Write-Host "[3/5] Preparando staging e assets do instalador / Windows 11 MSIX..." -ForegroundColor Yellow
    $InstallerDir = Join-Path $RepoRoot "installer"
    $StagingDir = Join-Path $InstallerDir "staging"
    $ShellexStage = Join-Path $StagingDir "shellex"
    $AssetsStage = Join-Path $ShellexStage "Assets"
    $DistDir = Join-Path $RepoRoot "dist"

    if (Test-Path $StagingDir) { Remove-Item $StagingDir -Recurse -Force }
    New-Item -ItemType Directory -Path $AssetsStage -Force | Out-Null
    New-Item -ItemType Directory -Path $DistDir -Force | Out-Null

    Copy-Item $ExePath (Join-Path $StagingDir "anycompare.exe") -Force
    Copy-Item $DllPath (Join-Path $StagingDir "AnyCompareShellEx64.dll") -Force
    Copy-Item $DllPath (Join-Path $ShellexStage "AnyCompareShellEx64.dll") -Force
    Copy-Item (Join-Path $InstallerDir "register_shellex.ps1") (Join-Path $StagingDir "register_shellex.ps1") -Force
    Copy-Item (Join-Path $InstallerDir "README_INSTALL.txt") (Join-Path $StagingDir "README_INSTALL.txt") -Force

    # Gerar AppxManifest.xml com versão sincronizada
    $manifestTpl = Get-Content (Join-Path $InstallerDir "shellex\AppxManifest.xml.in") -Raw
    $manifestXml = $manifestTpl.Replace("__APPX_VERSION__", $AppxVersion)
    [System.IO.File]::WriteAllText((Join-Path $ShellexStage "AppxManifest.xml"), $manifestXml, [System.Text.UTF8Encoding]::new($false))

    # Gerar Logo_44x44.png, Logo_150x150.png, WizardImage.bmp (164x314) e WizardSmallImage.bmp (55x58)
    $MainIconPng = Join-Path $RepoRoot "assets\icons\app.png"
    $pyGen = @"
from PIL import Image, ImageDraw
src = Image.open(r'$MainIconPng').convert('RGBA')

# MSIX logos (44x44 e 150x150)
src.resize((44, 44), Image.LANCZOS).save(r'$AssetsStage\Logo_44x44.png')
src.resize((150, 150), Image.LANCZOS).save(r'$AssetsStage\Logo_150x150.png')

# Inno Setup WizardImage (164x314, fundo claro BC5 azul-acinzentado com ícone centralizado)
wiz = Image.new('RGB', (164, 314), (240, 244, 249))
draw = ImageDraw.Draw(wiz)
for y in range(314):
    t = y / 313.0
    r = int(245 - 24 * t)
    g = int(248 - 18 * t)
    b = int(252 - 8 * t)
    draw.line([(0, y), (163, y)], fill=(r, g, b))
draw.rectangle([0, 0, 163, 4], fill=(0, 120, 212))
draw.line([(163, 0), (163, 313)], fill=(205, 214, 224))
icon_big = src.resize((96, 96), Image.LANCZOS)
wiz.paste(icon_big, (34, 72), icon_big)
wiz.save(r'$StagingDir\WizardImage.bmp', 'BMP')

# Inno Setup WizardSmallImage (55x58)
small = Image.new('RGB', (55, 58), (255, 255, 255))
icon_sm = src.resize((44, 44), Image.LANCZOS)
small.paste(icon_sm, (5, 7), icon_sm)
small.save(r'$StagingDir\WizardSmallImage.bmp', 'BMP')
"@
    python -c $pyGen
    if ($LASTEXITCODE -ne 0) { throw "Falha ao gerar assets gráficos do instalador." }

    # 4. Empacotar AnyCompareShellEx.msix com makeappx.exe do Windows SDK
    Write-Host "[4/5] Empacotando AnyCompareShellEx.msix (Windows 11 Context Menu)..." -ForegroundColor Yellow
    $KitsRoot = "C:\Program Files (x86)\Windows Kits\10\bin"
    $MakeAppx = $null
    $SignTool = $null
    if (Test-Path $KitsRoot) {
        $sdkDirs = Get-ChildItem $KitsRoot -Directory | Sort-Object Name -Descending
        foreach ($d in $sdkDirs) {
            $cand = Join-Path $d.FullName "x64\makeappx.exe"
            if (Test-Path $cand) {
                $MakeAppx = $cand
                $SignTool = Join-Path $d.FullName "x64\signtool.exe"
                break
            }
        }
    }

    $MsixOut = Join-Path $StagingDir "AnyCompareShellEx.msix"
    if ($MakeAppx) {
        & $MakeAppx pack /d $ShellexStage /p $MsixOut /o /nv | Out-Host
        if ($LASTEXITCODE -ne 0) { throw "makeappx.exe falhou ao empacotar AnyCompareShellEx.msix." }

        # Assinar automaticamente se um PFX foi informado ou existe em installer\certs\AnyCompare.pfx
        if (-not $CertPfx) {
            $defaultPfx = Join-Path $InstallerDir "certs\AnyCompare.pfx"
            if (Test-Path $defaultPfx) { $CertPfx = $defaultPfx }
        }
        if ($CertPfx -and (Test-Path $CertPfx) -and $SignTool -and (Test-Path $SignTool)) {
            Write-Host "    Assinando AnyCompareShellEx.msix com '$CertPfx'..." -ForegroundColor Cyan
            $signArgs = @("sign", "/fd", "SHA256", "/f", $CertPfx)
            if ($CertPassword) { $signArgs += @("/p", $CertPassword) }
            $signArgs += $MsixOut
            & $SignTool @signArgs | Out-Host
            if ($LASTEXITCODE -ne 0) { throw "signtool.exe falhou ao assinar o MSIX." }
        } else {
            Write-Host "    MSIX gerado sem assinatura (funciona via Modo de Desenvolvedor; passe -CertPfx para assinar)." -ForegroundColor DarkGray
        }
    } else {
        Write-Host "    AVISO: makeappx.exe não encontrado; seguindo com manifesto desempacotado + menu clássico." -ForegroundColor DarkYellow
    }

    # 5. Compilar instalador Inno Setup
    Write-Host "[5/5] Compilando instalador Inno Setup..." -ForegroundColor Yellow
    $IsccCandidates = @(
        "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
        "C:\Program Files\Inno Setup 6\ISCC.exe",
        "C:\Program Files (x86)\Inno Setup 5\ISCC.exe",
        "C:\Program Files\Inno Setup 5\ISCC.exe"
    )
    $Iscc = $IsccCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $Iscc) {
        throw "ISCC.exe (Inno Setup) não encontrado."
    }

    $IssPath = Join-Path $InstallerDir "AnyCompare.iss"
    & $Iscc "/DAppVersion=$AppVersion" "/DStagingDir=$StagingDir" "/DOutputDir=$DistDir" $IssPath | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "ISCC.exe falhou ao compilar o instalador." }

    $SetupExe = Join-Path $DistDir "AnyCompare-Setup-$AppVersion.exe"
    $sizeMb = [math]::Round((Get-Item $SetupExe).Length / 1MB, 2)
    Write-Host ""
    Write-Host "SUCESSO! Instalador gerado em:" -ForegroundColor Green
    Write-Host "  $SetupExe ($sizeMb MB)" -ForegroundColor Green
} finally {
    Pop-Location
}
