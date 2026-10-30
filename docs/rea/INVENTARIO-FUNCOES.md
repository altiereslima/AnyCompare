# Inventário de funções do Beyond Compare 4.4.7 por tipo de comparação

Gerado por `tools/build_function_inventory.py` a partir de `docs/rea/forms.json` (declarações `TUiAction` dos DFM, **CONFIRMADO ESTATICAMENTE**), `docs/rea/menus-composed.json` (árvores efetivas após herança e poda, ver `docs/rea/menus-composed.json` → `rule`) e `docs/review/coverage.json` (revisão estática do AnyCompare 0.1.5). Os status do AnyCompare vêm da revisão manual: presença no fonte não é equivalência funcional. Correções posteriores à revisão estão em `OVERRIDES` no gerador e aparecem com o texto "Correção 16/10/2026".

## Tipos de sessão

13 tipos criados dinamicamente por `TBaseActions` VMT+0x98 (0x1944900), com handlers `DirCompareNewExecute`…`XMLCompareNewExecute` (0x1942410–0x1942650). O módulo de ações de cada tipo é **INFERIDO** pelos nomes (lacuna `unk_8535d836…`).

| Tipo | Descrição | Módulo de ações (INFERIDO) |
|---|---|---|
| DirCompare | Comparação de pastas | TDirActions |
| DirMerge | Mesclagem de pastas | TDirActions |
| DirSync | Sincronização de pastas | TDirSyncActions |
| TextCompare | Comparação de texto | TTextActions |
| TextMerge | Mesclagem de texto | TTextActions |
| TableCompare | Comparação de tabelas | TDataActions |
| DataCompare | Comparação de dados (CSV/planilhas) | TDataActions |
| HexCompare | Comparação hexadecimal (binária) | THexActions |
| MP3Compare | Comparação de MP3 | TMP3Actions |
| PixCompare | Comparação de imagens | TPixActions |
| RegCompare | Comparação de registro | TRegActions |
| VerCompare | Comparação de versão (PE) | TVerActions |
| XMLCompare | Comparação de XML | TTextActions |

## Resumo por módulo

| Módulo | Escopo | Ações próprias | Presente | Parcial | Ausente | Fora do Windows | Menus no Windows |
|---|---|---|---|---|---|---|---|
| TBaseActions | Comuns a todas as janelas (Base) | 40 | 4 | 11 | 14 | 11 | Session, Tools, Help |
| THomeActions | Home | 4 | 0 | 2 | 1 | 0 | Session, View, Tools, Help |
| TSessionActions | Comuns às sessões | 16 | 1 | 9 | 6 | 0 | Session, View, Tools, Help |
| TFileActions | Comuns às comparações de arquivo | 58 | 0 | 22 | 35 | 1 | Session, File, Edit, Search, View, Tools, Help |
| TDirBaseActions | Comuns a pastas (DirBase) | 36 | 5 | 11 | 19 | 1 | Session, Actions, Edit, Search, View, Tools, Help |
| TDirActions | Comparação/mesclagem de pastas | 117 | 0 | 14 | 103 | 0 | Session, Actions, Edit, Search, View, Tools, Help |
| TDirSyncActions | Sincronização de pastas | 9 | 0 | 0 | 9 | 0 | Session, Actions, Edit, Search, View, Tools, Help |
| TTextActions | Texto (comparação, mesclagem, XML) | 114 | 17 | 18 | 78 | 1 | Session, File, Edit, Search, View, Tools, Help |
| THexActions | Hexadecimal / binário | 24 | 0 | 13 | 11 | 0 | Session, File, Edit, Search, View, Tools, Help |
| TDataActions | Tabela / dados | 30 | 0 | 0 | 30 | 0 | Session, File, Edit, Search, View, Tools, Help |
| TPixActions | Imagem | 18 | 0 | 0 | 18 | 0 | Session, File, Edit, Search, View, Tools, Help |
| TMP3Actions | MP3 | 4 | 0 | 0 | 4 | 0 | Session, File, Edit, Search, View, Tools, Help |
| TRegActions | Registro | 29 | 0 | 0 | 29 | 0 | Session, File, Edit, Search, View, Tools, Help |
| TVerActions | Versão | 2 | 0 | 0 | 2 | 0 | Session, File, Edit, Search, View, Tools, Help |

Cada módulo herda as ações dos ancestrais (`load_order`); as tabelas abaixo listam só as ações declaradas no próprio módulo. Os menus vêm da fusão herdada com poda: `Mac`, `MacTools`, `MacWindow` e `MergeFile` saem no Windows; Base/Home não têm barra de ferramentas.

## Comuns a todas as janelas (Base) — `TBaseActions`

Herança: TBaseActions. Menus (Windows): Session, Tools, Help. Barras: —. Popups: —.

| Ação | Legenda (EN) | Atalho | Categoria | O que faz | Onde aparece | AnyCompare |
|---|---|---|---|---|---|---|
| `MacCommandLineToolsAction` | Install Command Line Tools... |  | Mac | Places bcomp and bcompare in /usr/local/bin/ folder to make BCompare available from the terminal prompt. |  | fora_windows — Comando específico de macOS fora do alvo Windows. |
| `SessionNewListAction` | New Session |  | Session | Opens empty session of picked type in new view. | Menu Session | parcial — Home abre texto, pastas e hex; demais tipos continuam desabilitados. |
| `TabNewAction` | New Tab | Ctrl+T | Session | Opens Home view in new tab. | Menu Session | parcial — Ctrl+T cria uma comparação de texto; o original abre Home em nova aba. |
| `WindowNewAction` | New Window |  | Session | Opens Home view in new window. | Menu Session | parcial — Abre nova instância do executável próprio. |
| `SessionOpenAction` | Open Session | Shift+Ctrl+O | Session | Opens Home in new view. | Menu Session | parcial — Sessões e espaços de trabalho JSON locais preservam caminhos/opções e confirmam alterações antes de substituir sessões. Sem importação do formato BC4 nem gerenciador completo. |
| `WorkspaceLoadListAction` | Load Workspace |  | Session | Restores picked saved view configuration. | Menu Session | parcial — Sessões e espaços de trabalho JSON locais preservam caminhos/opções e confirmam alterações antes de substituir sessões. Sem importação do formato BC4 nem gerenciador completo. |
| `WorkspaceManagerAction` | Manage Workspaces... |  | Session | Manages the saved view configurations that can be restored. | Menu Session | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `WorkspaceSaveAsAction` | Save Workspace As... |  | Session | Stores current view configuration. | Menu Session | parcial — Sessões e espaços de trabalho JSON locais preservam caminhos/opções e confirmam alterações antes de substituir sessões. Sem importação do formato BC4 nem gerenciador completo. |
| `CloseTabAction` | Close Tab | Ctrl+W | Session | Closes active tab. | Menu Session | presente — Fecha aba/aplicação; percorre os arquivos alterados com Salvar/Não/Cancelar. Cancelar ou erro mantém a sessão. |
| `ExitAction` | Exit | Ctrl+Q | Session | Quits the application, prompts to save edits. | Menu Session | presente — Fecha aba/aplicação; percorre os arquivos alterados com Salvar/Não/Cancelar. Cancelar ou erro mantém a sessão. |
| `ProgramOptionsAction` | Options... |  | Tools | Sets global preferences. | Menu Tools | parcial — Opções locais incluem escolha de idioma automático/pt-BR/inglês; conjunto completo do original não reproduzido. |
| `FileFormatManagerAction` | File Formats... |  | Tools | Manages file formats. | Menu Tools | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ProfileManagerAction` | Profiles... |  | Tools | Manages profiles. | Menu Tools | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `SourceControlManagerAction` | Source Control Integration... |  | Tools | Manages integration with version control systems. | Menu Tools | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ExportAction` | Export Settings... |  | Tools | Saves user settings as a file. | Menu Tools | parcial — Exporta/importa/restaura configurações próprias; valida entrada, preserva backup e não manipula o perfil BC4. |
| `ImportAction` | Import Settings... |  | Tools | Loads user settings from a file. | Menu Tools | parcial — Exporta/importa/restaura configurações próprias; valida entrada, preserva backup e não manipula o perfil BC4. |
| `RestoreAction` | Restore Factory Defaults... |  | Tools | Loads factory default settings. | Menu Tools | parcial — Exporta/importa/restaura configurações próprias; valida entrada, preserva backup e não manipula o perfil BC4. |
| `SaveSnapshotAction` | Save Snapshot... |  | Tools | Saves details of specified folder for future comparison. | Menu Tools | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `EditTextFileAction` | Edit Text File |  | Tools | Opens new Text Edit view. | Menu Tools | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ViewPatchAction` | View Patch... |  | Tools | Opens picked patch file in new Text Patch view. | Menu Tools | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `RaiseExceptionAction` | Raise Exception |  | Tools | Raise Exception | Menu Tools | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `MacWindowMinimizeAction` | Minimize |  | MacWindow | Minimizes active window. |  | fora_windows — Comando específico de macOS fora do alvo Windows. |
| `MacWindowMinimizeAllAction` | Minimize All |  | MacWindow | Minimizes all application windows. |  | fora_windows — Comando específico de macOS fora do alvo Windows. |
| `MacWindowZoomAction` | Zoom |  | MacWindow | Maximizes or restores active window. |  | fora_windows — Comando específico de macOS fora do alvo Windows. |
| `MacWindowZoomAllAction` | Zoom All |  | MacWindow | Maximizes all visible document windows. |  | fora_windows — Comando específico de macOS fora do alvo Windows. |
| `MacWindowNextAction` | Cycle Through Windows |  | MacWindow | Selects window after active one. |  | fora_windows — Comando específico de macOS fora do alvo Windows. |
| `MacWindowPrevAction` | Cycle Back Through Windows |  | MacWindow | Selects window before active one. |  | fora_windows — Comando específico de macOS fora do alvo Windows. |
| `TabNextAction` |  | Ctrl+Tab | MacWindow | Selects tab after active one. |  | presente — Ctrl+Tab e Ctrl+Shift+Tab alternam as sessões abertas. |
| `TabPrevAction` |  | Shift+Ctrl+Tab | MacWindow | Selects tab before active one. |  | presente — Ctrl+Tab e Ctrl+Shift+Tab alternam as sessões abertas. |
| `MacTabToWindowAction` | Move Tab to New Window |  | MacWindow | Removes active tab and makes it into new window. |  | fora_windows — Comando específico de macOS fora do alvo Windows. |
| `MacWindowMergeAllAction` | Merge All Windows |  | MacWindow | Moves all tabs onto active window. |  | fora_windows — Comando específico de macOS fora do alvo Windows. |
| `MacWindowActivateAllAction` | Bring All to Front |  | MacWindow | Brings all windows to the foreground. |  | fora_windows — Comando específico de macOS fora do alvo Windows. |
| `MacWindowArrangeAllAction` | Arrange in Front |  | MacWindow | Arranges all visible windows. |  | fora_windows — Comando específico de macOS fora do alvo Windows. |
| `HelpContentsAction` | Contents |  | Help | Shows help file. | Menu Help | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ContextHelpAction` | Context Sensitive Help | F1 | Help | Shows context sensitive help for the active window. | Menu Help | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `VisitSiteAction` | Scooter Software on the Web |  | Help | Opens the Scooter Software website in your browser. | Menu Help | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `CheckForUpdatesAction` | Check for Updates |  | Help | Checks our website to see if a newer version is available. | Menu Help | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `SupportAction` | Support |  | Help | Displays Scooter Software contact information. | Menu Help | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `EnterKeyAction` | Enter Key... |  | Help | Enter registration key. | Menu Help | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `AboutAction` | About %s |  | Help | Displays program information, version number and copyright. | Menu Help | parcial — Sobre usa mensagem nativa com versão própria; não reproduz os 23 componentes da caixa original. |

## Home — `THomeActions`

Herança: TBaseActions → THomeActions. Menus (Windows): Session, View, Tools, Help. Barras: —. Popups: —.

| Ação | Legenda (EN) | Atalho | Categoria | O que faz | Onde aparece | AnyCompare |
|---|---|---|---|---|---|---|
| `FindAction` |  | Ctrl+F | Search | Finds search string. |  | parcial — Filtro do histórico próprio implementado; atalho Ctrl+F e gestão completa de sessões ausentes. |
| `ShowManageAction` | Show Session Management |  | View | Shows or hides management panel. | Menu View | parcial — Barra lateral própria sempre visível; não há alternância ou toda a gestão do original. |
| `MaximizeManageAction` | Maximize Session Management |  | View | Maximizes or restores management panel. | Menu View | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ShowWebAction` | Show Web Resources |  | View | Shows or hides web panel. | Menu View | excluida_usuario — Faixa vermelha de suporte, tutoriais e registro removida por instrução explícita de 14/10/2026. |

## Comuns às sessões — `TSessionActions`

Herança: TBaseActions → TSessionActions. Menus (Windows): Session, View, Tools, Help. Barras: Main. Popups: —.

| Ação | Legenda (EN) | Atalho | Categoria | O que faz | Onde aparece | AnyCompare |
|---|---|---|---|---|---|---|
| `SessionHomeAction` |  | Alt+Home | Session | Replaces current view with Home view. | Toolbar Main | presente — Página Inicial por menu/barra ou Alt+Home; mantém as sessões abertas. |
| `SessionsComboAction` |  |  | Session | Focuses on dropdown button listing sessions that can be opened in this view. | Toolbar Main | parcial — O botão Sessões volta à Home; não há popup com a árvore e todos os comandos do original. |
| `SessionSaveAction` | Save Session | Shift+Ctrl+S | Session | Saves this session. | Menu Session; Toolbar Main | parcial — Grava caminhos e opções da sessão em JSON próprio; alterações dos documentos continuam exigindo Salvar arquivo. |
| `SessionSaveAsAction` | Save Session As... |  | Session | Saves this session with new name. | Menu Session; Toolbar Main | parcial — Grava caminhos e opções da sessão em JSON próprio; alterações dos documentos continuam exigindo Salvar arquivo. |
| `SessionEditAction` | Session Settings... |  | Session | Edits current session settings. | Menu Session | parcial — Configuração nativa com regras básicas de caixa/espaços; gramáticas e regras avançadas ausentes. |
| `SessionLockedAction` | Locked |  | Session | Locks or unlocks current session. | Menu Session; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `SessionClearAction` | Clear Session | Shift+Ctrl+C | Session | Replaces current view with empty session. | Menu Session | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `RulesAction` |  |  | Session | Edits current Rules definitions. | Toolbar Main | parcial — Configuração nativa com regras básicas de caixa/espaços; gramáticas e regras avançadas ausentes. |
| `SwapAction` | Swap Sides |  | Session | Exchanges left, right files. | Menu Session; Toolbar Main | parcial — Inverte dois lados em texto/pastas/hex; sem sessão de três vias ou painel de saída. |
| `ReportAction` | %s Report... |  | Session | Produces %0:s report. | Menu Session; Toolbar Main | parcial — Relatórios HTML/TXT de texto/pastas; seleção/opções disponíveis não abrangem os layouts originais. |
| `InfoAction` | %s Info | Ctrl+I | Session | Displays %0:s statistics. | Menu Session; Toolbar Main | parcial — Informações da comparação de texto em diálogo nativo. |
| `CompareParentsAction` | Compare Parent Folders |  | Session | Compares folders containing current items in new view. | Menu Session | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `MergeParentsAction` | Merge Parent Folders |  | Session | Merges folders containing current items in new view. | Menu Session | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `InsigIgnoredAction` | Ignore Unimportant Differences |  | View | Treats unimportant differences as same. | Menu View; Toolbar Main | parcial — Ignora diferenças de caixa/espaços por opção; não todas as regras de importância do BC4. |
| `SameModIgnoredAction` | Ignore Same Changes |  | View | Treats same changes as same. | Menu View | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ShowToolbarAction` | Toolbar |  | View | Shows or hides toolbar. | Menu View | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |

## Comuns às comparações de arquivo — `TFileActions`

Herança: TBaseActions → TSessionActions → TFileActions. Menus (Windows): Session, File, Edit, Search, View, Tools, Help. Barras: Main. Popups: —.

Formulários de opções/diálogos do tipo (12): `TFILECODEPAGEERRORDIALOG`, `TFILECONFIRMSAVEDIALOG`, `TFILEFINDFRAME`, `TFILEFORMATCONVERSIONFRAME`, `TFILEFORMATFRAME`, `TFILEFORMATGENERALFRAME`, `TFILEFORMATMANAGERDIALOG`, `TFILEFORMATSAVEASDIALOG`, `TFILEREPORTDIALOG`, `TFILESESSIONFORMATSFRAME`, `TFILESESSIONSPECSFRAME`, `TFILEVIEWFRAME`.

| Ação | Legenda (EN) | Atalho | Categoria | O que faz | Onde aparece | AnyCompare |
|---|---|---|---|---|---|---|
| `FileFormatAction` |  |  | Session | Edits current File Format. | Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ReloadAction` | Reload Files | F5 | Session | Reloads displayed files. | Menu Session; Toolbar Main | parcial — Texto: F5 confirma alterações e relê disco. Não implementado em todos os comparadores. |
| `RecompareAction` | Recompare Files | Ctrl+F5 | Session | Recompares displayed files without reloading them. | Menu Session; Toolbar Main | parcial — Texto: Ctrl+F5 calcula comparação completa dos buffers sem reler disco. |
| `CompareUsingViewListAction` | Compare Files Using |  | Session | Compares files in picked new view. | Menu Session | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `MergeViewAction` | Merge Files |  | Session | Merges files in new view. | Menu Session; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `CompareToOutputListAction` | Compare to Output |  | Session | Compares picked side and output in new view. | Menu Session | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `CompareLeftToOutputAction` | Left Side |  | Session | Compares left and output in new view. | Menu Session | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `CompareCenterToOutputAction` | Center Side |  | Session | Compares center and output in new view. | Menu Session | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `CompareRightToOutputAction` | Right Side |  | Session | Compares right and output in new view. | Menu Session | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `OpenAction` | Open File... | Ctrl+O | File | Opens existing file into picked pane. | Menu File | parcial — Seleção nativa e caminhos locais implementados em texto; sem perfis remotos e todos os contextos do original. |
| `OpenLocalAction` | Open File on File System... |  | File | Opens existing local file into picked pane. | Menu File | parcial — Seleção nativa e caminhos locais implementados em texto; sem perfis remotos e todos os contextos do original. |
| `OpenProfileAction` | Open File Using Profile... |  | File | Opens existing file into picked pane using profile. | Menu File | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `OpenClipboardAction` | Open Clipboard | Shift+Ctrl+V | File | Opens Clipboard contents into picked pane. | Menu File | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `OpenWithListAction` | Open With |  | File | Opens picked file with picked application. | Menu File | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `AssocAppAction` | Associated Application |  | File | Opens picked file with associated application. | Menu File | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `RevealInFinderAction` | Reveal in Finder |  | File | Shows Finder and selects the picked file. | Menu File | fora_windows — Comando específico de macOS fora do alvo Windows. |
| `SaveAction` | Save File | Ctrl+S | File | Saves picked file which has been modified. | Menu File | parcial — Texto e binário editáveis com gravação explícita e conflito externo; texto preserva BOM/encoding/EOL, rejeita perda em ANSI e oferece codificação Unicode. |
| `SaveAsAction` | Save File As... | F12 | File | Saves picked file with new name. | Menu File | parcial — Texto e binário editáveis com gravação explícita e conflito externo; texto preserva BOM/encoding/EOL, rejeita perda em ANSI e oferece codificação Unicode. |
| `SaveAsLocalAction` | Save File on File System As... |  | File | Saves picked file with new name on file system. | Menu File | parcial — Texto e binário editáveis com gravação explícita e conflito externo; texto preserva BOM/encoding/EOL, rejeita perda em ANSI e oferece codificação Unicode. |
| `SaveAsProfileAction` | Save File Using Profile As... |  | File | Saves picked file with new name using profile. | Menu File | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ExplorerListAction` | Explorer |  | File | Shows Windows Explorer context menu for picked file. | Menu File | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `UndoAction` |  | Ctrl+Z | Edit | Undoes previous action. | Menu Edit; Toolbar Main | parcial — Texto: Ctrl+Z/Ctrl+Y/Ctrl+Shift+Z, histórico de até 20 estados de texto; não inclui todas as propriedades/operações do original. |
| `RedoAction` |  | Ctrl+Y | Edit | Redoes previously undone action. | Menu Edit; Toolbar Main | parcial — Texto: Ctrl+Z/Ctrl+Y/Ctrl+Shift+Z, histórico de até 20 estados de texto; não inclui todas as propriedades/operações do original. |
| `CopyAction` | Copy | Ctrl+C | Edit | Copies selection and puts it on Clipboard. | Menu Edit; Toolbar Main | parcial — Seleção/cópia em texto editável/leitura e no hexadecimal; contextos avançados não verificados. |
| `SelectAllAction` | Select All | Ctrl+A | Edit | Selects all visible items. | Menu Edit | parcial — Seleção/cópia em texto editável/leitura e no hexadecimal; contextos avançados não verificados. |
| `DiffItemNextAction` | Next Difference Item | Ctrl+N | Search | Positions at next difference item. | Menu Search; Toolbar Main | parcial — Texto navega por seções; hex tem navegação por byte na barra. Itens e seleções do original não são integralmente reproduzidos. |
| `DiffItemPrevAction` | Previous Difference Item | Ctrl+P | Search | Positions at previous difference item. | Menu Search; Toolbar Main | parcial — Texto navega por seções; hex tem navegação por byte na barra. Itens e seleções do original não são integralmente reproduzidos. |
| `DiffFileNextAction` | Next Difference Files | Ctrl+M | Search | Opens folder compare's next difference files. | Menu Search; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `DiffFilePrevAction` | Previous Difference Files | Shift+Ctrl+M | Search | Opens folder compare's previous difference files. | Menu Search; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `CopyFileRightAction` | Copy File to Right and Open Next Difference | Ctrl+Alt+R | Search | Copies left file to right and opens folder compare's next difference. | Menu Search; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `CopyFileLeftAction` | Copy File to Left and Open Next Difference | Ctrl+Alt+L | Search | Copies right file to left and opens folder compare's next difference. | Menu Search; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TakeFileLeftAction` | Take Left File and Open Next Difference | Ctrl+Alt+L | Search | Takes left file and opens folder merge's next difference. | Menu Search; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TakeFileRightAction` | Take Right File and Open Next Difference | Ctrl+Alt+R | Search | Takes right file and opens folder merge's next difference. | Menu Search; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ExistsNextAction` |  | Alt+Down | Search | Positions at next item, skipping over gaps. |  | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ExistsPrevAction` |  | Alt+Up | Search | Positions at previous item, skipping over gaps. |  | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `FindAction` | Find... | Ctrl+F | Search | Finds search string. | Menu Search; Toolbar Main | parcial — Texto: Unicode, regex, caixa, palavra inteira e ambos os lados; hexadecimal: procura bytes/UTF-8. |
| `FindNextAction` | Find Next | F3 | Search | Finds next occurrence of search string. | Menu Search; Toolbar Main | parcial — Texto: Unicode, regex, caixa, palavra inteira e ambos os lados; hexadecimal: procura bytes/UTF-8. |
| `FindPrevAction` | Find Previous | Shift+F3 | Search | Finds previous occurrence of search string. | Menu Search; Toolbar Main | parcial — Texto: Unicode, regex, caixa, palavra inteira e ambos os lados; hexadecimal: procura bytes/UTF-8. |
| `GoLeftAction` |  | Alt+Left | Search | Positions in pane left of current one. |  | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `GoRightAction` |  | Alt+Right | Search | Positions in pane right of current one. |  | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `GoNextAction` |  | F6 | Search | Positions in pane after current one. |  | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `GoPrevAction` |  | Shift+F6 | Search | Positions in pane before current one. |  | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `FavCompareAllAction` | Show All |  | Favorites | Shows same, left orphan, right orphan and difference items. | Menu View; Toolbar Main | parcial — Texto: filtros Todos/Diferenças/Iguais com faixas e expansão compartilhada; sem todos os comparadores/regras do original. |
| `FavDiffAction` | Show Differences |  | Favorites | Shows left orphan, right orphan and difference items. | Menu View; Toolbar Main | parcial — Texto: filtros Todos/Diferenças/Iguais com faixas e expansão compartilhada; sem todos os comparadores/regras do original. |
| `FavSameAction` | Show Same |  | Favorites | Shows same items. | Menu View; Toolbar Main | parcial — Texto: filtros Todos/Diferenças/Iguais com faixas e expansão compartilhada; sem todos os comparadores/regras do original. |
| `FavCompareNoneAction` | Show None |  | View | Shows no items. | Menu View; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TogSameAction` |  |  | Toggles | Shows or hides same items. | Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TogLeftOrphanAction` |  |  | Toggles | Shows or hides left orphan items. | Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TogRightOrphanAction` |  |  | Toggles | Shows or hides right orphan items. | Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TogDiffAction` |  |  | Toggles | Shows or hides difference items. | Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ShowWSAction` | Visible Whitespace |  | View | Shows or hides visible spaces, tabs. | Menu View; Toolbar Main | parcial — Alterna marcadores visuais de espaços/tabulações no texto sem mudar os buffers. |
| `DisplayFontAction` | Display Font... |  | View |  | Menu View; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `DisplayFontIncreaseAction` | Increase Display Font Size | Ctrl+Num + | View |  | Menu View; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `DisplayFontDecreaseAction` | Decrease Display Font Size | Ctrl+Num - | View |  | Menu View; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `DisplayFontResetAction` | Reset Display Font Size | Ctrl+Num 0 | View |  | Menu View; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `SideBySideAction` | Side-by-side Layout |  | View | Arranges panes left and right of each other. | Menu View; Toolbar Main | parcial — Layout lado a lado fixo; não há ação para alternar layouts. |
| `OverUnderAction` | Over-under Layout |  | View | Arranges panes above and below each other. | Menu View; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ShowSpecInfoAction` | File Info |  | View | Shows or hides file information. | Menu View; Toolbar Main | parcial — Cabeçalhos exibem data/tamanho/encoding/EOL, sempre visíveis; não há a opção completa de mostrar/ocultar informações. |

## Comuns a pastas (DirBase) — `TDirBaseActions`

Herança: TBaseActions → TSessionActions → TDirBaseActions. Menus (Windows): Session, Actions, Edit, Search, View, Tools, Help. Barras: Main. Popups: Tree.

| Ação | Legenda (EN) | Atalho | Categoria | O que faz | Onde aparece | AnyCompare |
|---|---|---|---|---|---|---|
| `SwapAction` |  |  |  | Exchanges left, right base folders. | Menu Session; Toolbar Main | presente — Inverte caminhos e inicia nova leitura, cancelando a anterior. |
| `BrowseListAction` | Browse for Folder |  | Session | Browses for an existing folder on picked side. | Menu Session | presente — Seletores de pastas locais nativos, com janela principal como proprietária. |
| `BrowseLeftAction` | Left Side |  | Session | Browses for an existing folder on left side. | Menu Session | presente — Seletores de pastas locais nativos, com janela principal como proprietária. |
| `BrowseCenterAction` | Center Side |  | Session | Browses for an existing folder on center side. | Menu Session | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `BrowseRightAction` | Right Side |  | Session | Browses for an existing folder on right side. | Menu Session | presente — Seletores de pastas locais nativos, com janela principal como proprietária. |
| `BrowseOutputAction` | Output Side |  | Session | Browses for an existing folder on output side. | Menu Session | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `StopAction` |  |  | Session | Terminates comparison. | Toolbar Main | presente — Cancela a comparação em segundo plano; resultados cancelados são identificados como parciais. |
| `ExpandAction` | Open Folder |  | Actions | Opens selected folder. | Menu Actions; PopupMenu Tree | parcial — Árvore de pastas com expansão/recolhimento; todos os contextos e seleções BC4 não verificados. |
| `CollapseAction` | Close Folder |  | Actions | Closes selected folder. | Menu Actions; PopupMenu Tree | parcial — Árvore de pastas com expansão/recolhimento; todos os contextos e seleções BC4 não verificados. |
| `ExpandSubfoldersAction` | Open Subfolders |  | Actions | Opens selected folders and all their subfolders. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `CollapseSubfoldersAction` | Close Subfolders |  | Actions | Closes selected folders and their subfolders. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `SetAsBaseFoldersAction` |  |  | Actions | Sets selected folders as base folders. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `SetAsOtherBaseFolderAction` | Set as Base on Other Side |  | Actions | Sets selected folder as base folder on other side. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `OpenAction` |  |  | Actions | Opens selected items in new view. | Menu Actions; PopupMenu Tree | parcial — Duplo clique abre somente pares de arquivos existentes; pastas/órfãos não têm navegação/abertura equivalentes. |
| `OpenWithListAction` | Open With |  | Actions | Opens selected items in new view of picked type. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `AssocAppAction` | Associated Application |  | Actions | Opens selected items with associated application. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `RevealInFinderAction` | Reveal in Finder |  | Actions | Shows Finder and selects the picked file. | Menu Actions; PopupMenu Tree | fora_windows — Comando específico de macOS fora do alvo Windows. |
| `ExcludeAction` | Exclude |  | Actions | Filters selected items to remove them from comparison. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `CopyFilenamesAction` | Copy Filename |  | Actions | Copies selected items' paths to Clipboard. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `RefreshSelectionAction` | Refresh Selection | Shift+F5 | Actions | Refreshes selected items. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `ExplorerListAction` | Explorer |  | Actions | Shows Windows Explorer context menu for selected items. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `ExpandAllAction` | Expand All |  | Edit | Opens all folders. | Menu Edit; Toolbar Main | parcial — Árvore de pastas com expansão/recolhimento; todos os contextos e seleções BC4 não verificados. |
| `CollapseAllAction` | Collapse All |  | Edit | Closes all folders. | Menu Edit; Toolbar Main | parcial — Árvore de pastas com expansão/recolhimento; todos os contextos e seleções BC4 não verificados. |
| `SelectAllAction` | Select All | Ctrl+A | Edit | Selects all visible items. | Menu Edit; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `SelectInvertAction` | Invert Selection |  | Edit | Selects all items not currently selected, and deselects all items currently selected. | Menu Edit | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `RefreshAction` | Refresh | F5 | Edit | Refreshes open folders. | Menu Edit; Toolbar Main | parcial — F5/Ctrl+F5 e Atualizar fazem nova varredura completa; não há distinção otimizada ou atualização por seleção. |
| `FullRefreshAction` | Full Refresh | Ctrl+F5 | Edit | Refreshes entire comparison. | Menu Edit; Toolbar Main | parcial — F5/Ctrl+F5 e Atualizar fazem nova varredura completa; não há distinção otimizada ou atualização por seleção. |
| `ExistsNextAction` |  | Alt+Down | Search | Positions at next filename, skipping over gaps. |  | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `ExistsPrevAction` |  | Alt+Up | Search | Positions at previous filename, skipping over gaps. |  | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `FindFilenameAction` | Find Filename... | Ctrl+F | Search | Finds filename matching search string. | Menu Search; Toolbar Main | parcial — Localiza próximo/anterior nome por substring com prompt nativo; sem sintaxe completa de máscaras BC4. |
| `FindFilenameNextAction` | Find Next Filename | F3 | Search | Finds next filename matching search string. | Menu Search; Toolbar Main | parcial — Localiza próximo/anterior nome por substring com prompt nativo; sem sintaxe completa de máscaras BC4. |
| `FindFilenamePrevAction` | Find Previous Filename | Shift+F3 | Search | Finds previous filename matching search string. | Menu Search; Toolbar Main | parcial — Localiza próximo/anterior nome por substring com prompt nativo; sem sintaxe completa de máscaras BC4. |
| `SuppressFiltersAction` | Suppress Filters |  | View | Temporarily suppresses display and file filters. | Menu View; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `ColListAction` | Columns |  | View | Shows or hides picked column. | Menu View | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `LegendAction` | Legend | Ctrl+Alt+L | View | Shows or hides legend window. | Menu View | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `ShowLogAction` | Log |  | View | Shows or hides log panel. | Menu View | parcial — Painel de log alternável; exportação e todos os controles originais ausentes. |

## Comparação/mesclagem de pastas — `TDirActions`

Herança: TBaseActions → TSessionActions → TDirBaseActions → TDirActions. Menus (Windows): Session, Actions, Edit, Search, View, Tools, Help. Barras: Main. Popups: Tree.

Formulários de opções/diálogos do tipo (43): `TDIRALIGNWITHITEMDIALOG`, `TDIRBASEVIEWFRAME`, `TDIRFINDDIALOG`, `TDIRLEGENDDIALOG`, `TDIRLOADERRORDIALOG`, `TDIRNAMEFILTERPRESETDIALOG`, `TDIROPERRORSDIALOG`, `TDIROPPROGRESSFRAME`, `TDIROTHERFILTERITEMDIALOG`, `TDIRREPORTDIALOG`, `TDIRSCRIPTDIALOG`, `TDIRSESSIONCOMPARISONFRAME`, `TDIRSESSIONHANDLINGFRAME`, `TDIRSESSIONMISCFRAME`, `TDIRSESSIONNAMEFILTERFRAME`, `TDIRSESSIONOTHERFILTERSFRAME`, `TDIRSESSIONSPECSFRAME`, `TDIRSYNCSESSIONHANDLINGFRAME`, `TDIRSYNCSESSIONOPSFRAME`, `TDIRSYNCVIEWFRAME`, `TDIRVIEWFRAME`, `TOPBASEDIALOG`, `TOPCOMPAREDIALOG`, `TOPCOMPARERESULTSDIALOG`, `TOPCONFIRMREPLACEFILEDIALOG`, `TOPCOPYDIALOG`, `TOPCOPYTODIALOG`, `TOPCOPYTOOUTPUTDIALOG`, `TOPDELETEDIALOG`, `TOPDOSATTRIBDIALOG`, `TOPENWITHITEMDIALOG`, `TOPEXCLUDEDIALOG`, `TOPFILTEREDDIALOG`, `TOPMERGEDIALOG`, `TOPMOVEBOTHDIALOG`, `TOPMOVEDIALOG`, `TOPMOVETODIALOG`, `TOPNEWFOLDERDIALOG`, `TOPRENAMEDIALOG`, `TOPSYNCDIALOG`, `TOPTIONSFRAME`, `TOPTOUCHDIALOG`, `TOPUNIXATTRIBDIALOG`.

| Ação | Legenda (EN) | Atalho | Categoria | O que faz | Onde aparece | AnyCompare |
|---|---|---|---|---|---|---|
| `ExpandAction` |  |  | Disjoint |  | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `CollapseAction` |  |  | Disjoint |  | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `ExpandSubfoldersAction` |  |  | Disjoint |  | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `CollapseSubfoldersAction` |  |  | Disjoint |  | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `NameFilterEditAction` |  |  | Session | Focuses on edit listing current name file filters. | Toolbar Main | parcial — Filtro simples por substring, não expressões *.*, exclusões, listas ou presets do original. |
| `NameFilterAction` |  |  | Session | Edits name file filters. | Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `SpecsBackAction` | Back | Alt+Left | Session | Changes to previous base folders. | Menu Session | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `SpecsForwardAction` | Forward | Alt+Right | Session | Changes to next base folders. | Menu Session | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `UpOneLevelListAction` | Up One Level |  | Session | Changes base folders on picked sides to their parent folders. | Menu Session | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `UpOneLevelLeftAction` | Left Side |  | Session | Changes left base folder to its parent folder. | Menu Session | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `UpOneLevelCenterAction` | Center Side |  | Session | Changes center base folder to its parent folder. | Menu Session | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `UpOneLevelRightAction` | Right Side |  | Session | Changes right base folder to its parent folder. | Menu Session | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `UpOneLevelOutputAction` | Output Side |  | Session | Changes output base folder to its parent folder. | Menu Session | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `UpOneLevelAllAction` |  |  | Session |  | Menu Session | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `MergeBaseViewAction` | Merge Base Folders |  | Session | Merges base folders in new view. | Menu Session; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `SyncBaseViewAction` | Sync Base Folders |  | Session | Synchronizes base folders in new view. | Menu Session; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `CompareToOutputListAction` | Compare to Output |  | Session | Compares picked side and output in new view. | Menu Session | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `CompareLeftToOutputAction` | Left Side |  | Session | Compares left and output in new view. | Menu Session | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `CompareCenterToOutputAction` | Center Side |  | Session | Compares center and output in new view. | Menu Session | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `CompareRightToOutputAction` | Right Side |  | Session | Compares right and output in new view. | Menu Session | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `QuickCompareAction` | Quick Compare |  | Actions | Quick compare selected files. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `CompareToAction` | Compare To... | F7 | Actions | Marks item for comparing in new view. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `AlignAction` | Align |  | Actions | Forces selected items to be aligned. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `AlignWithAction` | Align With... | F6 | Actions | Marks item for manual realignment. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `CompareAction` | Compare Contents |  | Actions | Determines overall comparison state for selected items. | Menu Actions; PopupMenu Tree; Toolbar Main | parcial — Compara conteúdo recursivo ou tamanho/data e abre um par por duplo clique; sem comparação da seleção. |
| `MergeAction` | Merge... |  | Actions | Performs automated merge of selected items. | Menu Actions; PopupMenu Tree; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `CopyRightAction` | Copy to Right | Ctrl+R | Actions | Copies selected items from left to right. | Menu Actions; PopupMenu Tree; Toolbar Main | parcial — Cópia local em segundo plano com prévia de sobrescrita, cancelamento e gravação temporária; ACLs/atributos completos e todos os perfis não reproduzidos. |
| `CopyLeftAction` | Copy to Left | Ctrl+L | Actions | Copies selected items from right to left. | Menu Actions; PopupMenu Tree; Toolbar Main | parcial — Cópia local em segundo plano com prévia de sobrescrita, cancelamento e gravação temporária; ACLs/atributos completos e todos os perfis não reproduzidos. |
| `CopyAction` | Copy to Side... |  | Actions | Prompts which side to copy selected items to. | Menu Actions; PopupMenu Tree; Toolbar Main | parcial — Cópia local em segundo plano com prévia de sobrescrita, cancelamento e gravação temporária; ACLs/atributos completos e todos os perfis não reproduzidos. |
| `CopySideAction` |  |  | Actions | Copies selected items to other side. | Menu Actions; PopupMenu Tree; Toolbar Main | parcial — Cópia local em segundo plano com prévia de sobrescrita, cancelamento e gravação temporária; ACLs/atributos completos e todos os perfis não reproduzidos. |
| `CopyToOutputAction` | Copy to Output... |  | Actions | Copies selected items to output folder. | Menu Actions; PopupMenu Tree; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `MoveRightAction` | Move to Right |  | Actions | Moves selected items from left to right. | Menu Actions; PopupMenu Tree; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `MoveLeftAction` | Move to Left |  | Actions | Moves selected items from right to left. | Menu Actions; PopupMenu Tree; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `MoveAction` | Move to Side... |  | Actions | Prompts which side to move selected items to. | Menu Actions; PopupMenu Tree; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `MoveSideAction` |  |  | Actions | Moves selected items to other side. | Menu Actions; PopupMenu Tree; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `MoveBothAction` | Exchange |  | Actions | Moves selected items to other side. | Menu Actions; PopupMenu Tree; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `CopyToAction` | Copy to Folder... |  | Actions | Copies selected items to another folder. | Menu Actions; PopupMenu Tree; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `MoveToAction` | Move to Folder... |  | Actions | Moves selected items to another folder. | Menu Actions; PopupMenu Tree; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `DeleteAction` | Delete |  | Actions | Deletes selected items. | Menu Actions; PopupMenu Tree; Toolbar Main | parcial — Exclusão confirmada para Lixeira do Windows; exclusão permanente não implementada. |
| `PermanentDeleteAction` |  | Shift+Del | Actions | Deletes selected items without moving them to the recycle bin. |  | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `RenameAction` | Rename | F2 | Actions | Renames selected items. | Menu Actions; PopupMenu Tree; Toolbar Main | parcial — Renomeia item local selecionado com validação do nome. |
| `AttributesAction` | Attributes... |  | Actions | Sets the DOS attributes of selected files. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `TouchAction` | Touch... |  | Actions | Sets timestamps of selected items. | Menu Actions; PopupMenu Tree; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `NewFolderAction` | New Folder... | Ins | Actions | Creates a new folder. | Menu Actions; PopupMenu Tree; Toolbar Main | parcial — Cria pasta local no lado ativo com validação do nome. |
| `IgnoredAction` | Ignored |  | Actions | Ignores or unignores differences in selected items. | Menu Actions; PopupMenu Tree; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `FileReportAction` | File Compare Report... |  | Actions | Produces file comparison report. | Menu Actions; PopupMenu Tree; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `SyncListAction` | Synchronize |  | Actions | Synchronizes by picked method. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `UpdateRightAction` | Update Right... |  | Actions | Synchronizes by copying newer and orphan items from left to right. | Menu Actions; PopupMenu Tree; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `UpdateLeftAction` | Update Left... |  | Actions | Synchronizes by copying newer and orphan items from right to left. | Menu Actions; PopupMenu Tree; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `UpdateBothAction` | Update Both... |  | Actions | Synchronizes by copying newer and orphan items from left to right and from right to left. | Menu Actions; PopupMenu Tree; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `MirrorRightAction` | Mirror to Right... | Shift+Ctrl+R | Actions | Synchronizes by replacing all different items on right, deleting any orphan items on right, and copying any left orphan items to right. | Menu Actions; PopupMenu Tree; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `MirrorLeftAction` | Mirror to Left... | Shift+Ctrl+L | Actions | Synchronizes by replacing all different items on left, deleting any orphan items on left, and copying any right orphan items to left. | Menu Actions; PopupMenu Tree; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `SourceControlListAction` | Source Control |  | Actions | Performs picked source control command. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `VcsAddAction` | Add to Source Control |  | Actions | Adds selected items to source control. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `VcsRemoveAction` | Remove from Source Control |  | Actions | Removes selected items from source control. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `VcsGetLatestAction` | Get Latest Version |  | Actions | Gets latest version of items from source control. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `VcsGetAction` | Get... |  | Actions | Gets particular version of items from source control. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `VcsCheckInAction` | Check In... |  | Actions | Checks item into source control. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `VcsCheckOutAction` | Check Out |  | Actions | Checks out item from source control. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `VcsUndoCheckOutAction` | Undo Check Out |  | Actions | Undoes checkout from source control. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `VcsDiffAction` | Show Differences |  | Actions | Shows source control differences. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `VcsHistoryAction` | Show History |  | Actions | Shows source control history. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `VcsPropertiesAction` | Show Properties |  | Actions | Shows source control properties. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `VcsRunAppAction` | Run Source Control Application |  | Actions | Runs source control application. | Menu Actions; PopupMenu Tree | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `SelectAllFilesAction` | Select All Files | Shift+Ctrl+A | Edit | Selects all visible files. | Menu Edit; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `SelectNewerListAction` | Select Newer |  | Edit | Selects all newer files on picked sides. | Menu Edit | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `SelectNewerLeftAction` | Left Side |  | Edit | Selects all newer files on the left. | Menu Edit | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `SelectNewerRightAction` | Right Side |  | Edit | Selects all newer files on the right. | Menu Edit | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `SelectNewerBothAction` | Both Sides |  | Edit | Selects all newer files on both sides. | Menu Edit | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `SelectOrphanListAction` | Select Orphans |  | Edit | Selects all orphan files on picked sides. | Menu Edit | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `SelectOrphanLeftAction` | Left Side |  | Edit | Selects all orphan files on the left. | Menu Edit | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `SelectOrphanRightAction` | Right Side |  | Edit | Selects all orphan files on the right. | Menu Edit | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `SelectOrphanBothAction` | Both Sides |  | Edit | Selects all orphan files on both sides. | Menu Edit | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `DiffNextAction` | Next Difference | Ctrl+N | Search | Positions at next difference. | Menu Search; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `DiffPrevAction` | Previous Difference | Ctrl+P | Search | Positions at previous difference. | Menu Search; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `FavCompareAllAction` | Show All |  | Favorites | Shows same, left orphan, right orphan, left newer, right newer and difference items. | Menu View; Toolbar Main | parcial — Árvore/lista local com estados agregados e filtros; alinhamento manual de nomes e filtros avançados ausentes. |
| `FavDiffAction` | Show Differences |  | DropDown | Shows left orphan, right orphan, left newer, right newer and difference items. | Menu View; Toolbar Main; Toolbar Main | parcial — Árvore/lista local com estados agregados e filtros; alinhamento manual de nomes e filtros avançados ausentes. |
| `FavSameAction` | Show Same |  | Favorites | Shows same items. | Menu View; Toolbar Main | parcial — Árvore/lista local com estados agregados e filtros; alinhamento manual de nomes e filtros avançados ausentes. |
| `FavNoOrphanAction` | Show No Orphans |  | DropDown | Shows same, left newer, right newer and difference items. | Menu View; Toolbar Main; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `FavDiffNoOrphanAction` | Show Differences but No Orphans |  | DropDown | Shows left newer, right newer and difference items. | Menu View; Toolbar Main; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `FavOrphanAction` | Show Orphans |  | DropDown | Shows left orphan and right orphan items. | Menu View; Toolbar Main; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `FavLeftNewerAction` | Show Left Newer |  | DropDown | Shows left newer items. | Menu View; Toolbar Main; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `FavRightNewerAction` | Show Right Newer |  | DropDown | Shows right newer items. | Menu View; Toolbar Main; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `FavLeftNewerOrphanAction` | Show Left Newer and Left Orphans |  | DropDown | Shows left orphan and left newer items. | Menu View; Toolbar Main; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `FavRightNewerOrphanAction` | Show Right Newer and Right Orphans |  | DropDown | Shows right orphan and right newer items. | Menu View; Toolbar Main; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `FavLeftOrphanAction` | Show Left Orphans |  | DropDown | Shows left orphan items. | Menu View; Toolbar Main; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `FavRightOrphanAction` | Show Right Orphans |  | DropDown | Shows right orphan items. | Menu View; Toolbar Main; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `FavCompareNoneAction` | Show None |  | View | Hides all files. | Menu View; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `FavMergeAllAction` | Show All |  | Favorites | Shows unchanged, same change, left add, right add, left delete, right delete, left change, right change, difference change and conflict items. | Menu View; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `FavChangedAction` | Show Changes |  | DropDown | Shows left add, right add, left delete, right delete, left change, right change and conflict items. | Menu View; Toolbar Main; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `FavConflictAction` | Show Conflicts |  | DropDown | Shows conflict items. | Menu View; Toolbar Main; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `FavLeftAction` | Show Left Changes |  | DropDown | Shows left add, left delete and left change items. | Menu View; Toolbar Main; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `FavRightAction` | Show Right Changes |  | DropDown | Shows right add, right delete and right change items. | Menu View; Toolbar Main; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `FavMergeableAction` | Show Mergeable |  | DropDown | Shows same change and difference change items. | Menu View; Toolbar Main; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `FavUnchangedAction` | Show Unchanged |  | Favorites | Shows unchanged items. | Menu View; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `FavMergeNoneAction` | Show None |  | View | Shows no items. | Menu View; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `TogSameAction` |  |  | Toggles | Shows or hides same items. | Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `TogLeftOrphanAction` |  |  | Toggles | Shows or hides left orphan items. | Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `TogRightOrphanAction` |  |  | Toggles | Shows or hides right orphan items. | Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `TogLeftNewerAction` |  |  | Toggles | Shows or hides left newer items. | Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `TogRightNewerAction` |  |  | Toggles | Shows or hides right newer items. | Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `TogDiffAction` |  |  | Toggles | Shows or hides difference items. | Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `TogNoModAction` |  |  | Toggles | Shows or hides unchanged items. | Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `TogSameModAction` |  |  | Toggles | Shows or hides same change items. | Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `TogLeftAddAction` |  |  | Toggles | Shows or hides left add items. | Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `TogRightAddAction` |  |  | Toggles | Shows or hides right add items. | Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `TogLeftDelAction` |  |  | Toggles | Shows or hides left delete items. | Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `TogRightDelAction` |  |  | Toggles | Shows or hides right delete items. | Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `TogLeftModAction` |  |  | Toggles | Shows or hides left change items. | Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `TogRightModAction` |  |  | Toggles | Shows or hides right change items. | Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `TogDiffModAction` |  |  | Toggles | Shows or hides mergeable items. | Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `TogConflictAction` |  |  | Toggles | Shows or hides conflict items. | Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `DirShowAllAction` | Always Show Folders |  | View | Shows all folders, ignoring the display filter. | Menu View; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `DirCompareStructureAction` | Compare Files and Folder Structure |  | View | Shows folders if they, or items they contain, match the display filter. | Menu View; Toolbar Main | parcial — Árvore/lista local com estados agregados e filtros; alinhamento manual de nomes e filtros avançados ausentes. |
| `DirCompareFilesOnlyAction` | Only Compare Files |  | View | Shows folders only if they contain files matching the display filter. | Menu View; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |
| `DirFlattenAction` | Ignore Folder Structure |  | View | Compares all files as if they were in a single folder. | Menu View; Toolbar Main | parcial — Árvore/lista local com estados agregados e filtros; alinhamento manual de nomes e filtros avançados ausentes. |
| `ShowCenterAction` | Center Pane |  | View | Shows or hides center pane. | Menu View; Toolbar Main | ausente — Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas. |

## Sincronização de pastas — `TDirSyncActions`

Herança: TBaseActions → TSessionActions → TDirBaseActions → TDirSyncActions. Menus (Windows): Session, Actions, Edit, Search, View, Tools, Help. Barras: Main. Popups: Tree.

Formulários de opções/diálogos do tipo (4): `TDIRSYNCSESSIONHANDLINGFRAME`, `TDIRSYNCSESSIONOPSFRAME`, `TDIRSYNCVIEWFRAME`, `TOPSYNCDIALOG`.

| Ação | Legenda (EN) | Atalho | Categoria | O que faz | Onde aparece | AnyCompare |
|---|---|---|---|---|---|---|
| `RunAction` |  |  | Session | Performs all sync operations. | Menu Session | ausente — Sincronização de pastas ainda não implementada; botão permanece desabilitado. |
| `CompareBaseViewAction` | Compare Base Folders |  | Session | Compares base folders in new view. | Menu Session; Toolbar Main | ausente — Sincronização de pastas ainda não implementada; botão permanece desabilitado. |
| `SyncLeaveAction` | Leave Alone |  | Actions | Changes sync operation to leave alone. | Menu Actions; PopupMenu Tree | ausente — Sincronização de pastas ainda não implementada; botão permanece desabilitado. |
| `SyncCopyLeftAction` | Copy Right to Left |  | Actions | Changes sync operation to copy right to left. | Menu Actions; PopupMenu Tree | ausente — Sincronização de pastas ainda não implementada; botão permanece desabilitado. |
| `SyncCopyRightAction` | Copy Left to Right |  | Actions | Changes sync operation to copy left to right. | Menu Actions; PopupMenu Tree | ausente — Sincronização de pastas ainda não implementada; botão permanece desabilitado. |
| `SyncDeleteLeftAction` | Delete Left |  | Actions | Changes sync operation to delete left. | Menu Actions; PopupMenu Tree | ausente — Sincronização de pastas ainda não implementada; botão permanece desabilitado. |
| `SyncDeleteRightAction` | Delete Right |  | Actions | Changes sync operation to delete right. | Menu Actions; PopupMenu Tree | ausente — Sincronização de pastas ainda não implementada; botão permanece desabilitado. |
| `ConflictNextAction` | Next Conflict | Ctrl+N | Search | Positions at next conflict. | Menu Search; Toolbar Main | ausente — Sincronização de pastas ainda não implementada; botão permanece desabilitado. |
| `ConflictPrevAction` | Previous Conflict | Ctrl+P | Search | Positions at previous conflict. | Menu Search; Toolbar Main | ausente — Sincronização de pastas ainda não implementada; botão permanece desabilitado. |

## Texto (comparação, mesclagem, XML) — `TTextActions`

Herança: TBaseActions → TSessionActions → TFileActions → TTextActions. Menus (Windows): Session, File, Edit, Search, View, Tools, Help. Barras: Main. Popups: Edit, LeftEdit, CenterEdit, RightEdit, OutputEdit, DetailsEdit, InputDetailsEdit, Gutter.

Formulários de opções/diálogos do tipo (15): `TTEXTFILESESSIONFORMATSFRAME`, `TTEXTFINDINFILESDIALOG`, `TTEXTFORMATCONVERSIONFRAME`, `TTEXTFORMATGRAMMARFRAME`, `TTEXTFORMATMISCFRAME`, `TTEXTGOTODIALOG`, `TTEXTGRAMMARITEMDIALOG`, `TTEXTINSIGITEMDIALOG`, `TTEXTREPLACEMENTITEMDIALOG`, `TTEXTREPORTDIALOG`, `TTEXTSESSIONALIGNMENTFRAME`, `TTEXTSESSIONIMPORTANCEFRAME`, `TTEXTSESSIONREPLACEMENTSFRAME`, `TTEXTVIEWFRAME`, `TTEXTWEIGHTITEMDIALOG`.

| Ação | Legenda (EN) | Atalho | Categoria | O que faz | Onde aparece | AnyCompare |
|---|---|---|---|---|---|---|
| `FileFormatAction` |  |  |  |  | Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `SelectAllAction` |  |  |  | Selects all visible lines. | Menu Edit | presente — Texto: Ctrl+A/Ctrl+X/Ctrl+V e menus; recortar/colar disponíveis em Editar, copiar também em leitura. Seleção segue o buffer real. |
| `ExistsNextAction` |  |  |  | Positions at next existing line, skipping over gaps. |  | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ExistsPrevAction` |  |  |  | Positions at previous existing line, skipping over gaps. |  | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `FavCompareAllAction` |  |  |  | Shows same, left orphan, right orphan and difference lines. | Menu View; Toolbar Main | presente — Filtros de comparação de dois textos, incluindo faixas +/− sincronizadas. Equivalência de regras avançadas não demonstrada. |
| `FavDiffAction` |  |  |  | Shows left orphan, right orphan and difference lines. | Menu View; Toolbar Main | presente — Filtros de comparação de dois textos, incluindo faixas +/− sincronizadas. Equivalência de regras avançadas não demonstrada. |
| `FavSameAction` |  |  |  | Shows same lines. | Menu View; Toolbar Main | presente — Filtros de comparação de dois textos, incluindo faixas +/− sincronizadas. Equivalência de regras avançadas não demonstrada. |
| `FavCompareNoneAction` |  |  |  | Shows no lines. | Menu View; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TogSameAction` |  |  |  | Shows or hides same lines. | Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TogLeftOrphanAction` |  |  |  | Shows or hides left orphan lines. | Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TogRightOrphanAction` |  |  |  | Shows or hides right orphan lines. | Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TogDiffAction` |  |  |  | Shows or hides difference lines. | Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `CheckInAction` | Check In... |  | File | Checks file into source control. | Menu Edit; Menu File | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `CheckOutAction` | Check Out |  | File | Checks out file from source control. | Menu Edit; Menu File | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `UndoCheckOutAction` | Undo Check Out |  | File | Undoes checkout from source control. | Menu Edit; Menu File | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ApplyPatchAction` | Apply Patch |  | File | Applies patch file. | Menu File | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `AlignWithAction` | Align With... | F7 | Edit | Marks item for manual realignment. | Menu Edit; PopupMenu CenterEdit; PopupMenu LeftEdit | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `IsolateAction` | Isolate |  | Edit | Realigns comparison so that selected lines are by themselves. | Menu Edit; PopupMenu CenterEdit; PopupMenu LeftEdit | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ReplacementAction` | Replacement... |  | Edit | Adds item to replacement list based on current difference. | Menu Edit | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `CopyRightAction` | Copy to Right | Ctrl+R | Edit | Copies selected lines or current section to right pane. | Menu Edit; PopupMenu Gutter; Toolbar Main | presente — Correção 16/10/2026: copia as linhas selecionadas no painel ativo ou, sem seleção, a seção atual (copy_selected → selected_rows + DiffEngine::merge_range, src/ui/text_compare.rs:395) na barra Copiar, em Ctrl+L/Ctrl+R e no menu Editar; o botão fica habilitado com seleção mesmo em linha igual. Linha parcialmente selecionada conta inteira; seleção que termina logo após a quebra de linha não inclui a linha seguinte; selecionar o documento inteiro (Ctrl+A) cobre também as linhas que só existem no outro lado antes da primeira e depois da última linha. Testes copy_* em text_compare.rs (Ctrl+A + Copiar para a Esquerda, seleção parcial, sem seleção, CRLF/BOM). Limites: a regra das lacunas nas bordas é inferida, não comparada em execução com o original; o popup da margem (Gutter) não existe no AnyCompare. Margem (17/10/2026): faixa de 35 px clicável inteira (medida no original a 100%); sem seleção, seta amarela 'Copiar Seção para …' por diferença; com seleção, uma única seta azul 'Copiar Linha(s) para …' na primeira linha selecionada, que copia só as linhas selecionadas (dicas do BCompare.tr; teste gutter_arrow_copies_only_the_selected_line_or_the_whole_section). |
| `CopyLeftAction` | Copy to Left | Ctrl+L | Edit | Copies selected lines or current section to left pane. | Menu Edit; PopupMenu Gutter; Toolbar Main | presente — Correção 16/10/2026: copia as linhas selecionadas no painel ativo ou, sem seleção, a seção atual (copy_selected → selected_rows + DiffEngine::merge_range, src/ui/text_compare.rs:395) na barra Copiar, em Ctrl+L/Ctrl+R e no menu Editar; o botão fica habilitado com seleção mesmo em linha igual. Linha parcialmente selecionada conta inteira; seleção que termina logo após a quebra de linha não inclui a linha seguinte; selecionar o documento inteiro (Ctrl+A) cobre também as linhas que só existem no outro lado antes da primeira e depois da última linha. Testes copy_* em text_compare.rs (Ctrl+A + Copiar para a Esquerda, seleção parcial, sem seleção, CRLF/BOM). Limites: a regra das lacunas nas bordas é inferida, não comparada em execução com o original; o popup da margem (Gutter) não existe no AnyCompare. Margem (17/10/2026): faixa de 35 px clicável inteira (medida no original a 100%); sem seleção, seta amarela 'Copiar Seção para …' por diferença; com seleção, uma única seta azul 'Copiar Linha(s) para …' na primeira linha selecionada, que copia só as linhas selecionadas (dicas do BCompare.tr; teste gutter_arrow_copies_only_the_selected_line_or_the_whole_section). |
| `CopySideAction` |  |  | Edit | Copies selected lines or current section to pane on other side. | Menu Edit; PopupMenu Gutter; Toolbar Main | presente — Correção 16/10/2026: copia as linhas selecionadas no painel ativo ou, sem seleção, a seção atual (copy_selected → selected_rows + DiffEngine::merge_range, src/ui/text_compare.rs:395) na barra Copiar, em Ctrl+L/Ctrl+R e no menu Editar; o botão fica habilitado com seleção mesmo em linha igual. Linha parcialmente selecionada conta inteira; seleção que termina logo após a quebra de linha não inclui a linha seguinte; selecionar o documento inteiro (Ctrl+A) cobre também as linhas que só existem no outro lado antes da primeira e depois da última linha. Testes copy_* em text_compare.rs (Ctrl+A + Copiar para a Esquerda, seleção parcial, sem seleção, CRLF/BOM). Limites: a regra das lacunas nas bordas é inferida, não comparada em execução com o original; o popup da margem (Gutter) não existe no AnyCompare. Margem (17/10/2026): faixa de 35 px clicável inteira (medida no original a 100%); sem seleção, seta amarela 'Copiar Seção para …' por diferença; com seleção, uma única seta azul 'Copiar Linha(s) para …' na primeira linha selecionada, que copia só as linhas selecionadas (dicas do BCompare.tr; teste gutter_arrow_copies_only_the_selected_line_or_the_whole_section). |
| `CopyItemRightAction` | Copy Line to Right | Shift+Ctrl+R | Edit | Copies current line to right pane. | Menu Edit; PopupMenu Gutter; Toolbar Main | parcial — Copia linha alinhada para o outro buffer; compatibilidade com todas as seleções do original não verificada. |
| `CopyItemLeftAction` | Copy Line to Left | Shift+Ctrl+L | Edit | Copies current line to left pane. | Menu Edit; PopupMenu Gutter; Toolbar Main | parcial — Copia linha alinhada para o outro buffer; compatibilidade com todas as seleções do original não verificada. |
| `CopyItemSideAction` |  |  | Edit | Copies current line to pane on other side. | Menu Edit; PopupMenu Gutter; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ConflictAction` | Conflict |  | Edit | Clears or sets conflict for selected lines or current section. | Menu Edit; PopupMenu CenterEdit; PopupMenu LeftEdit | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TakeLeftAction` | Take Left | Ctrl+L | Edit | Takes selected lines or current section from left pane. | Menu Edit; PopupMenu LeftEdit; PopupMenu OutputEdit | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TakeCenterAction` | Take Center |  | Edit | Takes selected lines or current section from center pane. | Menu Edit; PopupMenu CenterEdit; PopupMenu OutputEdit | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TakeRightAction` | Take Right | Ctrl+R | Edit | Takes selected lines or current section from right pane. | Menu Edit; PopupMenu OutputEdit; PopupMenu RightEdit | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TakeLeftRightAction` | Take Left Then Right | Ctrl+B | Edit | Takes selected lines or current section from left then right panes. | Menu Edit; PopupMenu CenterEdit; PopupMenu LeftEdit | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TakeRightLeftAction` | Take Right Then Left | Shift+Ctrl+B | Edit | Takes selected lines or current section from right then left panes. | Menu Edit; PopupMenu CenterEdit; PopupMenu LeftEdit | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TakeItemLeftAction` | Take Left Line | Shift+Ctrl+L | Edit | Takes current line from left pane. | Menu Edit; PopupMenu LeftEdit; PopupMenu OutputEdit | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TakeItemCenterAction` | Take Center Line |  | Edit | Takes current line from center pane. | Menu Edit; PopupMenu CenterEdit; PopupMenu OutputEdit | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TakeItemRightAction` | Take Right Line | Shift+Ctrl+R | Edit | Takes current line from right pane. | Menu Edit; PopupMenu OutputEdit; PopupMenu RightEdit | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `IncreaseIndentAction` | Increase Indent | Shift+Ctrl+I | Edit | Increases indentation of selected lines. | Menu Edit; PopupMenu OutputEdit | parcial — Operações de linhas reais com desfazer; sem todos os modos e seleções do original. |
| `DecreaseIndentAction` | Decrease Indent | Shift+Ctrl+U | Edit | Decreases indentation of selected lines. | Menu Edit; PopupMenu OutputEdit | parcial — Operações de linhas reais com desfazer; sem todos os modos e seleções do original. |
| `CutAction` | Cut | Ctrl+X | Edit | Cuts selection and puts it on Clipboard. | Menu Edit; PopupMenu DetailsEdit; PopupMenu OutputEdit | presente — Texto: Ctrl+A/Ctrl+X/Ctrl+V e menus; recortar/colar disponíveis em Editar, copiar também em leitura. Seleção segue o buffer real. |
| `PasteAction` | Paste | Ctrl+V | Edit | Inserts Clipboard contents. | Menu Edit; PopupMenu DetailsEdit; PopupMenu OutputEdit | presente — Texto: Ctrl+A/Ctrl+X/Ctrl+V e menus; recortar/colar disponíveis em Editar, copiar também em leitura. Seleção segue o buffer real. |
| `DeleteAction` | Delete |  | Edit | Deletes selection. | Menu Edit; PopupMenu DetailsEdit; PopupMenu OutputEdit | parcial — Widget de texto oferece exclusão/atalhos de palavras durante edição; menus/contextos específicos do original ausentes. |
| `DeleteItemAction` | Delete Line |  | Edit | Deletes current line. | Menu Edit; PopupMenu OutputEdit | parcial — Operações de linhas reais com desfazer; sem todos os modos e seleções do original. |
| `DeleteItemBeginAction` | Delete to Start of Line | Shift+Ctrl+BkSp | Edit | Deletes to start of current line. | Menu Edit; PopupMenu DetailsEdit; PopupMenu OutputEdit | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `DeleteItemEndAction` | Delete to End of Line | Shift+Ctrl+Del | Edit | Deletes to end of current line. | Menu Edit; PopupMenu DetailsEdit; PopupMenu OutputEdit | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `DeleteWordAction` | Delete Word |  | Edit | Deletes current word. | Menu Edit; PopupMenu DetailsEdit; PopupMenu OutputEdit | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `DeleteWordBeginAction` | Delete to Start of Word | Ctrl+BkSp | Edit | Deletes to start of current word. | Menu Edit; PopupMenu DetailsEdit; PopupMenu OutputEdit | parcial — Widget de texto oferece exclusão/atalhos de palavras durante edição; menus/contextos específicos do original ausentes. |
| `DeleteWordEndAction` | Delete to End of Word | Ctrl+Del | Edit | Deletes to end of current word. | Menu Edit; PopupMenu DetailsEdit; PopupMenu OutputEdit | parcial — Widget de texto oferece exclusão/atalhos de palavras durante edição; menus/contextos específicos do original ausentes. |
| `InsertItemBeforeAction` | Insert Line Before | Shift+Ctrl+Enter | Edit | Inserts new line before current one. | Menu Edit; PopupMenu OutputEdit | parcial — Operações de linhas reais com desfazer; sem todos os modos e seleções do original. |
| `InsertItemAfterAction` | Insert Line After | Ctrl+Enter | Edit | Inserts new line after current one. | Menu Edit; PopupMenu OutputEdit | parcial — Operações de linhas reais com desfazer; sem todos os modos e seleções do original. |
| `SelectSectionAction` | Select Section | Ctrl+D | Edit | Selects all lines in current section. | Menu Edit; PopupMenu CenterEdit; PopupMenu LeftEdit | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `CompareToClipboardAction` | Compare Selection to Clipboard |  | Edit | Compares selected text to clipboard in new view. | Menu Edit; PopupMenu CenterEdit; PopupMenu LeftEdit | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ConvertListAction` | Convert File |  | Edit | Performs picked file conversion. | Menu Edit | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TrimTrailWSAction` | Trim Trailing Whitespace |  | Edit | Strips off spaces and tab characters at the end of lines. | Menu Edit | parcial — Conversões de espaços/tabs no buffer com desfazer e gravação posterior explícita. |
| `LeadSpaceToTabAction` | Leading Spaces to Tabs |  | Edit | Replaces spaces at the beginning of lines with tab characters. | Menu Edit | parcial — Conversões de espaços/tabs no buffer com desfazer e gravação posterior explícita. |
| `TabToSpaceAction` | Tabs to Spaces |  | Edit | Replaces tab characters with spaces. | Menu Edit | parcial — Conversões de espaços/tabs no buffer com desfazer e gravação posterior explícita. |
| `EndingStyleListAction` | Line Endings |  | Edit | Converts line endings to picked style. | Menu Edit | presente — Seletor PC/Unix/Mac altera terminador de saída e estado dirty; misto preserva terminadores por linha. Conversão só grava ao salvar. |
| `ToWindowsAction` | To Windows/DOS |  | Edit | Converts line endings to Windows/DOS style. | Menu Edit | presente — Seletor PC/Unix/Mac altera terminador de saída e estado dirty; misto preserva terminadores por linha. Conversão só grava ao salvar. |
| `ToUnixAction` | To Unix |  | Edit | Converts line endings to Unix style. | Menu Edit | presente — Seletor PC/Unix/Mac altera terminador de saída e estado dirty; misto preserva terminadores por linha. Conversão só grava ao salvar. |
| `ToMacAction` | To Macintosh |  | Edit | Converts line endings to Macintosh style. | Menu Edit | presente — Seletor PC/Unix/Mac altera terminador de saída e estado dirty; misto preserva terminadores por linha. Conversão só grava ao salvar. |
| `FullEditAction` | Full Edit | F2 | Edit | Enables character-mode editing in the main file view. | Menu Edit; PopupMenu CenterEdit; PopupMenu LeftEdit | presente — F2 e Editar alternam mutabilidade sem remover cores ou alinhamento. Leitura permite seleção e cópia. |
| `ConflictSectionOffNextAction` | Clear Conflict Section, Next |  | Search | Clears conflict for current section and positions at next conflict section. | Menu Search; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ConflictSectionNextAction` | Next Conflict Section | Alt+N | Search | Positions at next conflict section. | Menu Search; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ConflictSectionPrevAction` | Previous Conflict Section | Alt+P | Search | Positions at previous conflict section. | Menu Search; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `DiffNextAction` | Next Difference | Shift+Ctrl+N | Search | Positions at next difference text. | Menu Search | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `DiffPrevAction` | Previous Difference | Shift+Ctrl+P | Search | Positions at previous difference text. | Menu Search | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `DiffSectionNextAction` | Next Difference Section | Ctrl+N | Search | Positions at next unignored difference section. | Menu Search; Toolbar Main | presente — Ctrl+N/Ctrl+P e barra navegam por seções contíguas. Botões desabilitam nas bordas. |
| `DiffSectionPrevAction` | Previous Difference Section | Ctrl+P | Search | Positions at previous unignored difference section. | Menu Search; Toolbar Main | presente — Ctrl+N/Ctrl+P e barra navegam por seções contíguas. Botões desabilitam nas bordas. |
| `DiffReplacedNextAction` | Next Replacement | Shift+Ctrl+Alt+N | Search | Positions at next replacement text. | Menu Search | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `DiffReplacedPrevAction` | Previous Replacement | Shift+Ctrl+Alt+P | Search | Positions at previous replacement text. | Menu Search | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TakenLeftNextAction` | Next Left Taken |  | Search | Positions at next range of lines where left was taken. | Menu Search | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TakenLeftPrevAction` | Previous Left Taken |  | Search | Positions at previous range of lines where left was taken. | Menu Search | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TakenRightNextAction` | Next Right Taken |  | Search | Positions at next range of lines where right was taken. | Menu Search | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TakenRightPrevAction` | Previous Right Taken |  | Search | Positions at previous range of lines where right was taken. | Menu Search | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `EditNextAction` | Next Edit |  | Search | Positions at next edited text. | Menu Search | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `EditPrevAction` | Previous Edit |  | Search | Positions at previous edited text. | Menu Search | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ReplaceAction` | Replace... | Ctrl+H | Search | Replaces search string. | Menu Search; Toolbar Main | parcial — Substitui ocorrência/todas com undo, regex e seleção de lados no texto. |
| `FindInFilesAction` | Find in Files... | Shift+Ctrl+F | Search | Finds search string in multiple files. | Menu Search | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `MacFindUseSelAction` | Use Selection for Find |  | Search | Finds selected text. | Menu Search | fora_windows — Comando específico de macOS fora do alvo Windows. |
| `GoToAction` | Go To... | Ctrl+G | Search | Positions at specified line and column. | Menu Search; Toolbar Main | parcial — Prompt nativo linha:coluna com revelação de linhas filtradas. |
| `BookmarkToggleListAction` | Toggle Bookmark |  | Search | Sets or clears picked bookmark. | Menu Search; PopupMenu Gutter | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `BookmarkGoToListAction` | Go to Bookmark |  | Search | Positions at picked bookmark. | Menu Search; PopupMenu Gutter | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `BookmarkClearAction` | Clear Bookmarks |  | Search | Removes all bookmarks. | Menu Search; PopupMenu Gutter | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `GoInputAction` |  | Alt+PgUp | Search | Positions in input pane. |  | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `GoOutputAction` |  | Alt+PgDn | Search | Positions in output pane. |  | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `FavMergeAllAction` | Show All |  | Favorites | Shows unchanged, same change, left change, right change, different change and conflict lines. | Menu View; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `FavChangedAction` | Show Changes |  | DropDown | Shows same change, left change, right change, different change and conflict lines. | Menu View; Toolbar Main; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `FavConflictAction` | Show Conflicts |  | DropDown | Shows conflict lines. | Menu View; Toolbar Main; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `FavLeftAction` | Show Left Changes |  | DropDown | Shows same change, left change, different change and conflict lines. | Menu View; Toolbar Main; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `FavRightAction` | Show Right Changes |  | DropDown | Shows same change, right change, different change and conflict lines. | Menu View; Toolbar Main; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `FavMergeableAction` | Show Mergeable |  | DropDown | Shows same change, left change, right change and different change lines. | Menu View; Toolbar Main; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `FavUnchangedAction` | Show Unchanged |  | Favorites | Shows unchanged lines. | Menu View; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `FavMergeNoneAction` | Show None |  | View | Shows no lines. | Menu View; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TogNoModAction` |  |  | Toggles | Shows or hides unchanged lines. | Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TogSameModAction` |  |  | Toggles | Shows or hides same change lines. | Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TogLeftModAction` |  |  | Toggles | Shows or hides left change lines. | Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TogRightModAction` |  |  | Toggles | Shows or hides right change lines. | Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TogDiffModAction` |  |  | Toggles | Shows or hides different change lines. | Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `TogConflictAction` |  |  | Toggles | Shows or hides conflict lines. | Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ShowContextAction` | Show Context |  | View | Shows lines surrounding difference. | Menu View; Toolbar Main | parcial — Mostra três linhas de contexto por lado; número de linhas não configurável. |
| `FavorLeftAction` | Favor Left Changes |  | View | Suppresses highlighting of left-only changes in output. | Menu View; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `FavorRightAction` | Favor Right Changes |  | View | Suppresses highlighting of right-only changes in output. | Menu View; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `IgnoredAction` | Ignored |  | View | Ignores or unignores differences in selected lines or current section. | Menu Edit; Menu View; PopupMenu CenterEdit | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ShowItemNumAction` | Line Numbers |  | View | Shows or hides line numbers. | Menu View; PopupMenu Gutter; Toolbar Main | presente — Opção de números das linhas no menu Visualizar. |
| `ShowSyntaxHighlightingAction` | Syntax Highlighting |  | View | Shows or hides syntax highlighting. | Menu View; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `OutputWindowAction` | Detached Output Pane |  | View | Shows output pane in separate window. | Menu View | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ZoomAction` |  | F11 | View | Makes current pane larger. |  | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ZoomLeftAction` |  | Ctrl+Alt+Right | View | Makes left pane incrementally larger. |  | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ZoomRightAction` |  | Ctrl+Alt+Left | View | Makes right pane incrementally larger. |  | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `WebBrowserAction` | Webpages |  | View | Shows or hides webpage panels. | Menu View; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ShowThumbnailAction` | Thumbnail |  | View | Shows or hides thumbnail view. | Menu View; Toolbar Main | parcial — Alterna miniatura e detalhes; configuração completa dos painéis do original não reproduzida. |
| `ShowCenterAction` | Center Pane |  | View | Shows or hides center pane. | Menu View; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ShowTextDetailsAction` | Text Details |  | View | Shows or hides text details panel. | Menu View; Toolbar Main | parcial — Alterna miniatura e detalhes; configuração completa dos painéis do original não reproduzida. |
| `ShowHexDetailsAction` | Hex Details |  | View | Shows or hides hexadecimal details panel. | Menu View; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ShowAlignDetailsAction` | Alignment Details |  | View | Shows or hides alignment details panel. | Menu View; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `ShowRulerAction` | Ruler |  | View | Shows or hides ruler in details panel. | Menu View; Toolbar Main | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |
| `OverwritingAction` |  | Ins | View | Typing replaces characters instead of inserting them. |  | ausente — Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento. |

## Hexadecimal / binário — `THexActions`

Herança: TBaseActions → TSessionActions → TFileActions → THexActions. Menus (Windows): Session, File, Edit, Search, View, Tools, Help. Barras: Main. Popups: Edit.

Formulários de opções/diálogos do tipo (6): `THEXFINDFRAME`, `THEXGOTODIALOG`, `THEXLAYOUTDIALOG`, `THEXREPORTDIALOG`, `THEXSESSIONFRAME`, `THEXVIEWFRAME`.

| Ação | Legenda (EN) | Atalho | Categoria | O que faz | Onde aparece | AnyCompare |
|---|---|---|---|---|---|---|
| `FileFormatAction` |  |  |  |  | Toolbar Main | ausente — Comando hexadecimal sem vínculo de implementação nesta auditoria; o módulo já permite edição e salvamento. |
| `SelectAllAction` |  |  |  | Selects all visible bytes. | Menu Edit | ausente — Comando hexadecimal sem vínculo de implementação nesta auditoria; o módulo já permite edição e salvamento. |
| `DiffItemNextAction` | Next Difference Bytes |  |  | Positions at next difference bytes. | Menu Search; Toolbar Main | parcial — Navegação por bytes diferentes e atalhos; granularidade e estados do original não foram integralmente verificados. |
| `DiffItemPrevAction` | Previous Difference Bytes |  |  | Positions at previous difference bytes. | Menu Search; Toolbar Main | parcial — Navegação por bytes diferentes e atalhos; granularidade e estados do original não foram integralmente verificados. |
| `ExistsNextAction` |  |  |  | Positions at next existing byte, skipping over gaps. |  | ausente — Comando hexadecimal sem vínculo de implementação nesta auditoria; o módulo já permite edição e salvamento. |
| `ExistsPrevAction` |  |  |  | Positions at previous existing byte, skipping over gaps. |  | ausente — Comando hexadecimal sem vínculo de implementação nesta auditoria; o módulo já permite edição e salvamento. |
| `FavCompareAllAction` |  |  |  | Shows same, left orphan, right orphan and difference bytes. | Menu View; Toolbar Main | ausente — Comando hexadecimal sem vínculo de implementação nesta auditoria; o módulo já permite edição e salvamento. |
| `FavDiffAction` |  |  |  | Shows left orphan, right orphan and difference bytes. | Menu View; Toolbar Main | ausente — Comando hexadecimal sem vínculo de implementação nesta auditoria; o módulo já permite edição e salvamento. |
| `FavSameAction` |  |  |  | Shows same bytes. | Menu View; Toolbar Main | ausente — Comando hexadecimal sem vínculo de implementação nesta auditoria; o módulo já permite edição e salvamento. |
| `CopyRightAction` | Copy to Right | Ctrl+R | Edit | Copies selected or current bytes to right pane. | Menu Edit; Toolbar Main | parcial — Copia intervalo selecionado entre documentos binários; não todos os modos de seleção BC4. |
| `CopyLeftAction` | Copy to Left | Ctrl+L | Edit | Copies selected or current bytes to left pane. | Menu Edit; Toolbar Main | parcial — Copia intervalo selecionado entre documentos binários; não todos os modos de seleção BC4. |
| `CopySideAction` |  |  | Edit | Copies selected or current bytes to pane on other side. | Menu Edit; Toolbar Main | parcial — Copia intervalo selecionado entre documentos binários; não todos os modos de seleção BC4. |
| `CutAction` | Cut | Ctrl+X | Edit | Cuts selection and puts it on Clipboard. | Menu Edit; Toolbar Main | parcial — Edição binária, clipboard e undo; edição/atalhos disponíveis não equivalem a todos os comandos de menu originais. |
| `PasteAction` | Paste | Ctrl+V | Edit | Inserts Clipboard contents. | Menu Edit; Toolbar Main | parcial — Edição binária, clipboard e undo; edição/atalhos disponíveis não equivalem a todos os comandos de menu originais. |
| `DeleteAction` | Delete |  | Edit | Deletes selection. | Menu Edit; Toolbar Main | parcial — Edição binária, clipboard e undo; edição/atalhos disponíveis não equivalem a todos os comandos de menu originais. |
| `ReplaceAction` | Replace... | Ctrl+H | Search | Replaces search string. | Menu Search; Toolbar Main | ausente — Comando hexadecimal sem vínculo de implementação nesta auditoria; o módulo já permite edição e salvamento. |
| `GoToAction` | Go To... | Ctrl+G | Search | Positions at specified byte. | Menu Search; Toolbar Main | parcial — Ir para offset com prompt nativo e validação. |
| `ItemNumAction` |  |  | View | Shows current byte address. | Menu Edit; Menu View | ausente — Comando hexadecimal sem vínculo de implementação nesta auditoria; o módulo já permite edição e salvamento. |
| `LittleEndianListAction` | Little Endian Values |  | View | Displays little endian values in picked way. | Menu Edit; Menu View | parcial — Opções de layout/endereço e endian no hexadecimal próprio; equivalência visual completa não verificada. |
| `BigEndianListAction` | Big Endian Values |  | View | Displays big endian values in picked way. | Menu Edit; Menu View | parcial — Opções de layout/endereço e endian no hexadecimal próprio; equivalência visual completa não verificada. |
| `ShowItemNumAction` | Byte Addresses |  | View | Shows or hides byte addresses. | Menu View; Toolbar Main | parcial — Opções de layout/endereço e endian no hexadecimal próprio; equivalência visual completa não verificada. |
| `LayoutAction` | Layout... |  | View | Configures layout options. | Menu View | parcial — Opções de layout/endereço e endian no hexadecimal próprio; equivalência visual completa não verificada. |
| `ShowThumbnailAction` | Thumbnail |  | View | Shows or hides thumbnail view. | Menu View; Toolbar Main | ausente — Comando hexadecimal sem vínculo de implementação nesta auditoria; o módulo já permite edição e salvamento. |
| `OverwritingAction` |  | Ins | View | Typing replaces bytes instead of inserting them. |  | ausente — Comando hexadecimal sem vínculo de implementação nesta auditoria; o módulo já permite edição e salvamento. |

## Tabela / dados — `TDataActions`

Herança: TBaseActions → TSessionActions → TFileActions → TDataActions. Menus (Windows): Session, File, Edit, Search, View, Tools, Help. Barras: Main. Popups: Edit, DetailsEdit.

Formulários de opções/diálogos do tipo (11): `TDATACOLHANDLINGDIALOG`, `TDATACOLINSERTDIALOG`, `TDATAFORMATCONVERSIONFRAME`, `TDATAFORMATREGIONALFRAME`, `TDATAFORMATTYPEFRAME`, `TDATAGOTODIALOG`, `TDATAREPORTDIALOG`, `TDATASESSIONALIGNMENTFRAME`, `TDATASESSIONFRAME`, `TDATASHOWCOLSDIALOG`, `TDATAVIEWFRAME`.

| Ação | Legenda (EN) | Atalho | Categoria | O que faz | Onde aparece | AnyCompare |
|---|---|---|---|---|---|---|
| `FileFormatAction` |  |  |  |  | Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `SelectAllAction` |  |  |  | Selects all visible rows. | Menu Edit | ausente — Comparador correspondente ainda não implementado. |
| `DiffItemNextAction` | Next Difference Row |  |  | Positions at next difference row. | Menu Search; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `DiffItemPrevAction` | Previous Difference Row |  |  | Positions at previous difference row. | Menu Search; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `ExistsNextAction` |  |  |  | Positions at next existing row, skipping over gaps. |  | ausente — Comparador correspondente ainda não implementado. |
| `ExistsPrevAction` |  |  |  | Positions at previous existing row, skipping over gaps. |  | ausente — Comparador correspondente ainda não implementado. |
| `FavCompareAllAction` |  |  |  | Shows same, left orphan, right orphan and difference rows. | Menu View; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `FavDiffAction` |  |  |  | Shows left orphan, right orphan and difference rows. | Menu View; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `FavSameAction` |  |  |  | Shows same rows. | Menu View; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `FavCompareNoneAction` |  |  |  | Shows no rows. | Menu View; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `TogSameAction` |  |  |  | Shows or hides same rows. | Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `TogLeftOrphanAction` |  |  |  | Shows or hides left orphan rows. | Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `TogRightOrphanAction` |  |  |  | Shows or hides right orphan rows. | Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `TogDiffAction` |  |  |  | Shows or hides difference rows. | Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `CopyRightAction` | Copy to Right | Ctrl+R | Edit | Copies selection to right pane. | Menu Edit; PopupMenu DetailsEdit; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `CopyLeftAction` | Copy to Left | Ctrl+L | Edit | Copies selection to left pane. | Menu Edit; PopupMenu DetailsEdit; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `CopySideAction` |  |  | Edit | Copies selection to pane on other side. | Menu Edit; PopupMenu DetailsEdit; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `CutAction` | Cut | Ctrl+X | Edit | Cuts selection and puts it on Clipboard. | Menu Edit; PopupMenu DetailsEdit; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `PasteAction` | Paste | Ctrl+V | Edit | Inserts Clipboard contents. | Menu Edit; PopupMenu DetailsEdit; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `DeleteAction` | Delete |  | Edit | Deletes selection. | Menu Edit; PopupMenu DetailsEdit; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `InsertAction` | Insert | Ins | Edit | Inserts new row. | Menu Edit | ausente — Comparador correspondente ainda não implementado. |
| `EditAction` | Edit | F2 | Edit | Edits cell text. | Menu Edit; PopupMenu DetailsEdit | ausente — Comparador correspondente ainda não implementado. |
| `EditNextAction` | Next Edit |  | Search | Positions at next edited text. | Menu Search | ausente — Comparador correspondente ainda não implementado. |
| `EditPrevAction` | Previous Edit |  | Search | Positions at previous edited text. | Menu Search | ausente — Comparador correspondente ainda não implementado. |
| `GoToAction` | Go To... | Ctrl+G | Search | Positions at specified row and column. | Menu Search; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `ResizeColsToFitAction` | Resize Columns to Fit |  | View | Makes columns wide enough for data to be fully visible. | Menu View | ausente — Comparador correspondente ainda não implementado. |
| `ShowColsAction` | Columns... |  | View | Shows or hides columns. | Menu View | ausente — Comparador correspondente ainda não implementado. |
| `ShowItemNumAction` | Row Numbers |  | View | Shows or hides row numbers. | Menu View; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `ShowThumbnailAction` | Thumbnail |  | View | Shows or hides thumbnail view. | Menu View; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `ShowDetailsAction` | Text Details |  | View | Shows or hides text details panel. | Menu View; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |

## Imagem — `TPixActions`

Herança: TBaseActions → TSessionActions → TFileActions → TPixActions. Menus (Windows): Session, File, Edit, Search, View, Tools, Help. Barras: Main. Popups: ScrollBox, CenterScrollBox, Edit.

Formulários de opções/diálogos do tipo (5): `TPIXFULLSCREENDIALOG`, `TPIXREPLACEMENTITEMDIALOG`, `TPIXREPORTDIALOG`, `TPIXSESSIONREPLACEMENTSFRAME`, `TPIXVIEWFRAME`.

| Ação | Legenda (EN) | Atalho | Categoria | O que faz | Onde aparece | AnyCompare |
|---|---|---|---|---|---|---|
| `FileFormatAction` |  |  |  |  | Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `DiffToleranceAction` | Tolerance Mode |  | View | Detects pixel differences greater than configurable tolerance. | Menu View; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `DiffRangeAction` | Mismatch Range Mode |  | View | Measures degree of pixel difference. | Menu View; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `DiffBlendAction` | Blend Mode |  | View | Blends pictures based on configurable percentage. | Menu View; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `AutoScaleAction` | Auto Scale |  | View | Makes smaller picture as large as other picture. | Menu View; PopupMenu ScrollBox; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `RotateRightAction` | Rotate Clockwise |  | View | Spins current picture to right by 90 degrees. | Menu View; PopupMenu ScrollBox; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `RotateLeftAction` | Rotate Counterclockwise |  | View | Spins current picture to left by 90 degrees. | Menu View; PopupMenu ScrollBox; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `FlipHorzAction` | Flip Horizontally |  | View | Swaps pixels of current picture relative to its horizontal center. | Menu View; PopupMenu ScrollBox; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `FlipVertAction` | Flip Vertically |  | View | Swaps pixels of current picture relative to its vertical center. | Menu View; PopupMenu ScrollBox; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `OffsetResetAction` | Reset Difference Offset |  | View | Realigns top left corner of both pictures. | Menu View; PopupMenu CenterScrollBox | ausente — Comparador correspondente ainda não implementado. |
| `CompareTagsAction` | Compare Metadata |  | View | Compares and shows metadata. | Menu View; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `BlendToggleAction` | Blend Toggle | F8 | View | Switches between left and right pictures. | Menu View | ausente — Comparador correspondente ainda não implementado. |
| `FullScreenAction` | Full Screen | F11 | View | Makes current picture show as large as possible. | Menu View; PopupMenu CenterScrollBox; PopupMenu ScrollBox | ausente — Comparador correspondente ainda não implementado. |
| `ZoomListAction` | Zoom |  | View | Makes picture appear to change size in picked way. | Menu View | ausente — Comparador correspondente ainda não implementado. |
| `ZoomInAction` | Zoom In |  | View | Makes pictures appear to be larger. | Menu View | ausente — Comparador correspondente ainda não implementado. |
| `ZoomOutAction` | Zoom Out |  | View | Makes pictures appear to be smaller. | Menu View | ausente — Comparador correspondente ainda não implementado. |
| `ZoomActualAction` | Actual Size |  | View | Makes pictures appear in their actual size. | Menu View | ausente — Comparador correspondente ainda não implementado. |
| `ZoomFitAction` | Fit to Window |  | View | Makes pictures appear to fill panes. | Menu View | ausente — Comparador correspondente ainda não implementado. |

## MP3 — `TMP3Actions`

Herança: TBaseActions → TSessionActions → TFileActions → TMP3Actions. Menus (Windows): Session, File, Edit, Search, View, Tools, Help. Barras: Main. Popups: Edit, DetailsEdit.

Formulários de opções/diálogos do tipo (2): `TMP3REPORTDIALOG`, `TMP3VIEWFRAME`.

| Ação | Legenda (EN) | Atalho | Categoria | O que faz | Onde aparece | AnyCompare |
|---|---|---|---|---|---|---|
| `Play2Action` |  |  | Session | Plays both audio tracks. | Menu Session; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `PlayAction` |  |  | File | Plays audio track. | Menu Edit; Menu File | ausente — Comparador correspondente ainda não implementado. |
| `IsSigAction` |  |  | View | Marks selected item as important or unimportant. | Menu Edit; Menu View | ausente — Comparador correspondente ainda não implementado. |
| `ShowDetailsAction` | Text Details |  | View | Shows or hides text details panel. | Menu View; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |

## Registro — `TRegActions`

Herança: TBaseActions → TSessionActions → TFileActions → TRegActions. Menus (Windows): Session, File, Edit, Search, View, Tools, Help. Barras: Main. Popups: Edit, DetailsEdit.

Formulários de opções/diálogos do tipo (6): `TBROWSEREGISTRYDIALOG`, `TREGBROWSEDIALOG`, `TREGFINDFRAME`, `TREGREPORTDIALOG`, `TREGVALDIALOG`, `TREGVIEWFRAME`.

| Ação | Legenda (EN) | Atalho | Categoria | O que faz | Onde aparece | AnyCompare |
|---|---|---|---|---|---|---|
| `SwapAction` |  |  |  | Exchanges left, right registries. | Menu Session; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `ReloadAction` | Reload |  |  | Reloads displayed registries. | Menu Session; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `RecompareAction` | Recompare |  |  | Recompares displayed registries without reloading them. | Menu Session; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `OpenAction` | Open... |  |  | Opens registry into picked pane. | Menu Edit; Menu File | ausente — Comparador correspondente ainda não implementado. |
| `OpenLocalAction` | Open Registry File on File System... |  |  | Opens existing local registry file into picked pane. | Menu Edit; Menu File | ausente — Comparador correspondente ainda não implementado. |
| `OpenProfileAction` | Open Registry File Using Profile... |  |  | Opens existing registry file into picked pane using profile. | Menu Edit; Menu File | ausente — Comparador correspondente ainda não implementado. |
| `UpOneLevelListAction` | Up One Level |  | Session | Changes base keys on picked sides to their parent keys. | Menu Session | ausente — Comparador correspondente ainda não implementado. |
| `UpOneLevelLeftAction` | Left Side |  | Session | Changes left base key to its parent key. | Menu Session | ausente — Comparador correspondente ainda não implementado. |
| `UpOneLevelRightAction` | Right Side |  | Session | Changes right base key to its parent key. | Menu Session | ausente — Comparador correspondente ainda não implementado. |
| `UpOneLevelBothAction` | Both Sides |  | Session | Changes both base keys to their parent keys. | Menu Session | ausente — Comparador correspondente ainda não implementado. |
| `OpenRegAction` | Open Registry... |  | File | Opens live registry into picked pane. | Menu Edit; Menu File | ausente — Comparador correspondente ainda não implementado. |
| `SetAsBasesAction` |  |  | Edit | Sets selected keys as base keys. | Menu Edit | ausente — Comparador correspondente ainda não implementado. |
| `SetBothAsBasesAction` | Set Both as Base Keys |  | Edit | Sets selected key and key on other side as base keys. | Menu Edit | ausente — Comparador correspondente ainda não implementado. |
| `SetAsOtherBaseAction` | Set as Base Key on Other Side |  | Edit | Sets selected key as base key on other side. | Menu Edit | ausente — Comparador correspondente ainda não implementado. |
| `CopyRightAction` | Copy to Right | Ctrl+R | Edit | Copies selected items to right pane. | Menu Edit; PopupMenu DetailsEdit; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `CopyLeftAction` | Copy to Left | Ctrl+L | Edit | Copies selected items to left pane. | Menu Edit; PopupMenu DetailsEdit; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `CopySideAction` |  |  | Edit | Copies selected items to pane on other side. | Menu Edit; PopupMenu DetailsEdit; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `DeleteAction` | Delete |  | Edit | Deletes selected items. | Menu Edit; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `RenameAction` | Rename | F2 | Edit | Renames item. | Menu Edit; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `NewKeyAction` | New Key |  | Edit | Adds a new key. | Menu Edit | ausente — Comparador correspondente ainda não implementado. |
| `NewValAction` | New Value | Ins | Edit | Adds a new value. | Menu Edit | ausente — Comparador correspondente ainda não implementado. |
| `ModifyAction` | Modify... |  | Edit | Modifies value. | Menu Edit; PopupMenu DetailsEdit | ausente — Comparador correspondente ainda não implementado. |
| `CopyKeyNameAction` | Copy Key Name |  | Edit | Copies key's name to Clipboard. | Menu Edit | ausente — Comparador correspondente ainda não implementado. |
| `ExportKeyAction` | Export... |  | Edit | Saves key as registry file. | Menu Edit | ausente — Comparador correspondente ainda não implementado. |
| `ExportAllAction` | Export All... |  | Edit | Saves all keys as registry file. | Menu Edit | ausente — Comparador correspondente ainda não implementado. |
| `ExpandAllAction` | Expand All |  | View | Opens all items. | Menu View; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `CollapseAllAction` | Collapse All |  | View | Closes all items. | Menu View; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `ShowTextDetailsAction` | Text Details |  | View | Shows or hides text details panel. | Menu View; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
| `ShowHexDetailsAction` | Hex Details |  | View | Shows or hides hexadecimal details panel. | Menu View; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |

## Versão — `TVerActions`

Herança: TBaseActions → TSessionActions → TFileActions → TVerActions. Menus (Windows): Session, File, Edit, Search, View, Tools, Help. Barras: Main. Popups: Edit, DetailsEdit.

Formulários de opções/diálogos do tipo (2): `TVERREPORTDIALOG`, `TVERVIEWFRAME`.

| Ação | Legenda (EN) | Atalho | Categoria | O que faz | Onde aparece | AnyCompare |
|---|---|---|---|---|---|---|
| `IsSigAction` |  |  | View | Marks selected item as important or unimportant. | Menu Edit; Menu View | ausente — Comparador correspondente ainda não implementado. |
| `ShowDetailsAction` | Text Details |  | View | Shows or hides text details panel. | Menu View; Toolbar Main | ausente — Comparador correspondente ainda não implementado. |
