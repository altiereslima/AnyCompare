; AnyCompare — Instalador Inno Setup (compatível com Inno Setup 5 e 6)
; Compilado automaticamente por tools\build_installer.ps1 sempre que o app é compilado.

#ifndef AppVersion
  #define AppVersion "0.2.16"
#endif
#ifndef StagingDir
  #define StagingDir "staging"
#endif
#ifndef OutputDir
  #define OutputDir "..\dist"
#endif
#ifndef IconFile
  #define IconFile "..\assets\icons\app.ico"
#endif

[Setup]
AppId={{A4E8C7D1-93B2-4F5A-8E11-5B9C3D7A2F00}
AppName=AnyCompare
AppVerName=AnyCompare {#AppVersion}
AppVersion={#AppVersion}
VersionInfoVersion={#AppVersion}.0
VersionInfoProductVersion={#AppVersion}.0
VersionInfoDescription=AnyCompare {#AppVersion} Setup
AppPublisher=AnyCompare
DefaultDirName={autopf}\AnyCompare
DefaultGroupName=AnyCompare
DisableProgramGroupPage=yes
AllowNoIcons=yes
PrivilegesRequired=admin
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=yes
RestartApplications=yes
OutputDir={#OutputDir}
OutputBaseFilename=AnyCompare-Setup-{#AppVersion}
SetupIconFile={#IconFile}
UninstallDisplayIcon={app}\anycompare.exe
UninstallDisplayName=AnyCompare {#AppVersion}
Compression=lzma2/max
SolidCompression=yes
ChangesAssociations=yes
ShowLanguageDialog=auto
WizardImageFile={#StagingDir}\WizardImage.bmp
WizardSmallImageFile={#StagingDir}\WizardSmallImage.bmp

[Languages]
Name: "ptbr"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "de"; MessagesFile: "compiler:Languages\German.isl"
Name: "es"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "fr"; MessagesFile: "compiler:Languages\French.isl"
Name: "it"; MessagesFile: "compiler:Languages\Italian.isl"
Name: "ja"; MessagesFile: "compiler:Languages\Japanese.isl"
Name: "ko"; MessagesFile: "compiler:Languages\Korean.isl"
Name: "ru"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "sl"; MessagesFile: "compiler:Languages\Slovenian.isl"

[CustomMessages]
ptbr.IncludeShellEx=Incluir no menu de &contexto do Windows Explorer
en.IncludeShellEx=Include in Windows &Explorer context menu
de.IncludeShellEx=In das Windows-&Explorer-Kontextmenü aufnehmen
es.IncludeShellEx=Incluir en el menú contextual del &Explorador de Windows
fr.IncludeShellEx=Inclure dans le menu contextuel de l'&Explorateur Windows
it.IncludeShellEx=Includi nel menu contestuale di &Esplora file di Windows
ja.IncludeShellEx=Windows エクスプローラーのコンテキストメニューに追加(&E)
ko.IncludeShellEx=Windows 탐색기 컨텍스트 메뉴에 포함(&E)
ru.IncludeShellEx=Добавить в контекстное меню &Проводника Windows
sl.IncludeShellEx=Vključi v priročni meni &Raziskovalca Windows

ptbr.AssocSessions=Associar arquivos de sessão (.acsession.json, .acworkspace.json)
en.AssocSessions=Associate session files (.acsession.json, .acworkspace.json)
de.AssocSessions=Sitzungsdateien verknüpfen (.acsession.json, .acworkspace.json)
es.AssocSessions=Asociar archivos de sesión (.acsession.json, .acworkspace.json)
fr.AssocSessions=Associer les fichiers de session (.acsession.json, .acworkspace.json)
it.AssocSessions=Associa i file di sessione (.acsession.json, .acworkspace.json)
ja.AssocSessions=セッションファイルを関連付ける (.acsession.json, .acworkspace.json)
ko.AssocSessions=세션 파일 연결 (.acsession.json, .acworkspace.json)
ru.AssocSessions=Связать файлы сессий (.acsession.json, .acworkspace.json)
sl.AssocSessions=Poveži datoteke sej (.acsession.json, .acworkspace.json)

ptbr.InstallingShellEx=Registrando extensão do menu de contexto do Windows Explorer...
en.InstallingShellEx=Registering Windows Explorer context menu extension...
de.InstallingShellEx=Windows-Explorer-Kontextmenüerweiterung wird registriert...
es.InstallingShellEx=Registrando la extensión del menú contextual del Explorador de Windows...
fr.InstallingShellEx=Enregistrement de l'extension du menu contextuel de l'Explorateur Windows...
it.InstallingShellEx=Registrazione dell'estensione del menu contestuale di Esplora file...
ja.InstallingShellEx=Windows エクスプローラーのコンテキストメニュー拡張を登録しています...
ko.InstallingShellEx=Windows 탐색기 컨텍스트 메뉴 확장을 등록하는 중...
ru.InstallingShellEx=Регистрация расширения контекстного меню Проводника Windows...
sl.InstallingShellEx=Registriranje razširitve priročnega menija Raziskovalca Windows...

ptbr.AdditionalTasks=Integração com o Windows:
en.AdditionalTasks=Windows integration:
de.AdditionalTasks=Windows-Integration:
es.AdditionalTasks=Integración con Windows:
fr.AdditionalTasks=Intégration Windows :
it.AdditionalTasks=Integrazione con Windows:
ja.AdditionalTasks=Windows 統合:
ko.AdditionalTasks=Windows 통합:
ru.AdditionalTasks=Интеграция с Windows:
sl.AdditionalTasks=Integracija z Windows:

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "shellextension"; Description: "{cm:IncludeShellEx}"; GroupDescription: "{cm:AdditionalTasks}"
Name: "fileassoc"; Description: "{cm:AssocSessions}"; GroupDescription: "{cm:AdditionalTasks}"; Flags: unchecked

[Files]
Source: "{#StagingDir}\anycompare.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#StagingDir}\AnyCompareShellEx64.dll"; DestDir: "{app}"; Flags: ignoreversion restartreplace uninsrestartdelete
Source: "{#StagingDir}\AnyCompareShellEx.msix"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#StagingDir}\register_shellex.ps1"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#StagingDir}\README_INSTALL.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#StagingDir}\shellex\*"; DestDir: "{app}\shellex"; Flags: ignoreversion recursesubdirs createallsubdirs

[Registry]
; Caminhos da aplicação (lidos pela extensão de menu de contexto AnyCompareShellEx64.dll)
; Root: HKA grava em HKLM quando instalado como Administrador (todos os usuários) ou HKCU para o usuário atual.
Root: HKA; Subkey: "Software\AnyCompare"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\AnyCompare"; ValueType: string; ValueName: "ExePath"; ValueData: "{app}\anycompare.exe"
Root: HKA; Subkey: "Software\AnyCompare"; ValueType: string; ValueName: "Version"; ValueData: "{#AppVersion}"
Root: HKA; Subkey: "Software\Microsoft\Windows\CurrentVersion\App Paths\anycompare.exe"; ValueType: string; ValueData: "{app}\anycompare.exe"; Flags: uninsdeletekey

; Associação opcional de sessões/espaços de trabalho salvos
Root: HKA; Subkey: "Software\Classes\.acsession"; ValueType: string; ValueData: "AnyCompare.Session"; Tasks: fileassoc; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\AnyCompare.Session"; ValueType: string; ValueData: "AnyCompare Session"; Tasks: fileassoc; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\AnyCompare.Session\DefaultIcon"; ValueType: string; ValueData: "{app}\anycompare.exe,0"; Tasks: fileassoc
Root: HKA; Subkey: "Software\Classes\AnyCompare.Session\shell\open\command"; ValueType: string; ValueData: """{app}\anycompare.exe"" ""%1"""; Tasks: fileassoc

[Icons]
Name: "{autoprograms}\AnyCompare"; Filename: "{app}\anycompare.exe"
Name: "{autodesktop}\AnyCompare"; Filename: "{app}\anycompare.exe"; Tasks: desktopicon

[Run]
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File ""{app}\register_shellex.ps1"" -Action Install -InstallDir ""{app}"""; StatusMsg: "{cm:InstallingShellEx}"; Tasks: shellextension; Flags: runhidden waituntilterminated runasoriginaluser

[UninstallRun]
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File ""{app}\register_shellex.ps1"" -Action Uninstall -InstallDir ""{app}"""; RunOnceId: "UninstallAnyCompareShellEx"; Flags: runhidden waituntilterminated

[UninstallDelete]
Type: filesandordirs; Name: "{app}\shellex"
