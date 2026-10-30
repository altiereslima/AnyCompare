"""Build a conservative source review of every recovered action/form.

This is an authored coverage assessment, not REA behavioral verification. A recovered
declaration is not necessarily an independent command (inherited overrides and other
platforms occur). Presence in our code does not prove equivalence with Beyond Compare.
"""
from collections import Counter
from datetime import date
import hashlib
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
REFERENCE = ROOT / "docs/reference/ui-inventory.json"
OUT = ROOT / "docs/review"
LABELS = {
    "presente": "Presente no escopo descrito",
    "parcial": "Parcial / divergente",
    "ausente": "Ausente",
    "fora_windows": "Específica de outra plataforma",
    "excluida_usuario": "Removida a pedido do usuário",
    "recurso": "Recurso / módulo de declarações",
}
sources = {str(p.relative_to(ROOT)).replace("\\", "/"): p.read_text(encoding="utf-8")
           for p in sorted((ROOT / "src").rglob("*.rs"))}


def location(path, anchor):
    source = sources[path]
    start = source.find(anchor)
    if start < 0:
        raise ValueError(f"Source anchor missing: {path}: {anchor}")
    return {"path": path, "line": source.count("\n", 0, start) + 1, "anchor": anchor}


assessments = {}


def mark(form, names, state, note, path, anchor):
    for name in names.split():
        key = (form, name)
        if key in assessments:
            raise ValueError(f"Duplicate assessment: {key}")
        assessments[key] = {"status": state, "assessment": note,
                            "source": location(path, anchor)}


MAIN = "src/main.rs"
TEXT = "src/ui/text_compare.rs"
FOLDER = "src/ui/folder_compare.rs"
HEX = "src/ui/hex_compare.rs"
HOME = "src/ui/home.rs"
DIALOGS = "src/ui/dialogs.rs"

mark("TBASEACTIONS", "SessionNewListAction", "parcial", "Home abre texto, pastas e hex; demais tipos continuam desabilitados.", HOME, "let items =")
mark("TBASEACTIONS", "TabNewAction", "parcial", "Ctrl+T cria uma comparação de texto; o original abre Home em nova aba.", MAIN, "egui::Key::T")
mark("TBASEACTIONS", "CloseTabAction ExitAction", "presente", "Fecha aba/aplicação; percorre os arquivos alterados com Salvar/Não/Cancelar. Cancelar ou erro mantém a sessão.", MAIN, "if self.pending_exit {")
mark("TBASEACTIONS", "TabNextAction TabPrevAction", "presente", "Ctrl+Tab e Ctrl+Shift+Tab alternam as sessões abertas.", MAIN, "let backwards =")
mark("TBASEACTIONS", "AboutAction", "parcial", "Sobre usa mensagem nativa com versão própria; não reproduz os 23 componentes da caixa original.", DIALOGS, "fn about(&mut self) {")
mark("TSESSIONACTIONS", "SessionHomeAction", "presente", "Página Inicial por menu/barra ou Alt+Home; mantém as sessões abertas.", MAIN, "egui::Key::Home")
mark("TSESSIONACTIONS", "SessionsComboAction", "parcial", "O botão Sessões volta à Home; não há popup com a árvore e todos os comandos do original.", TEXT, '"imgSessionsCombo"')
mark("TSESSIONACTIONS", "SwapAction", "parcial", "Inverte dois lados em texto/pastas/hex; sem sessão de três vias ou painel de saída.", TEXT, "fn swap_sides")

for names, state, note, anchor in [
    ("OpenAction OpenLocalAction", "parcial", "Seleção nativa e caminhos locais implementados em texto; sem perfis remotos e todos os contextos do original.", "pub fn open_side"),
    ("SaveAction", "parcial", "Texto: Ctrl+S salva só o lado ativo alterado. Ícone azul apenas quando dirty; sem edição/salvamento binário ou demais módulos.", "pub fn save_side"),
    ("SaveAsAction SaveAsLocalAction", "parcial", "Texto: F12 e seletor nativo Salvar como; codificação e PC/Unix preservados. Demais comparadores não editam.", "pub fn save_side"),
    ("ReloadAction", "parcial", "Texto: F5 confirma alterações e relê disco. Não implementado em todos os comparadores.", "fn reload"),
    ("RecompareAction", "parcial", "Texto: Ctrl+F5 calcula comparação completa dos buffers sem reler disco.", "fn recompare"),
    ("UndoAction RedoAction", "parcial", "Texto: Ctrl+Z/Ctrl+Y/Ctrl+Shift+Z, histórico de até 20 estados de texto; não inclui todas as propriedades/operações do original.", "fn redo_edit"),
    ("CopyAction SelectAllAction", "parcial", "Texto: seleção/cópia nativas em Editar e somente leitura; menus Ctrl+C/Ctrl+A. Outros comparadores ainda sem seleção de conteúdo.", '"Selecionar tudo  Ctrl+A"'),
    ("FavCompareAllAction FavDiffAction FavSameAction", "parcial", "Texto: filtros Todos/Diferenças/Iguais com faixas e expansão compartilhada; sem todos os comparadores/regras do original.", "fn diff_view"),
    ("DiffItemNextAction DiffItemPrevAction", "parcial", "Texto navega por seções; hex tem navegação por byte na barra. Itens e seleções do original não são integralmente reproduzidos.", "fn next_diff"),
    ("SideBySideAction", "parcial", "Layout lado a lado fixo; não há ação para alternar layouts.", "fn diff_view"),
    ("ShowSpecInfoAction", "parcial", "Cabeçalhos exibem data/tamanho/encoding/EOL, sempre visíveis; não há a opção completa de mostrar/ocultar informações.", "metadata_label"),
]:
    mark("TFILEACTIONS", names, state, note, TEXT, anchor)

for names, state, note, anchor in [
    ("FavCompareAllAction FavDiffAction FavSameAction", "presente", "Filtros de comparação de dois textos, incluindo faixas +/− sincronizadas. Equivalência de regras avançadas não demonstrada.", "fn diff_view"),
    ("CopyRightAction CopyLeftAction", "presente", "Ctrl+R/Ctrl+L copiam a seção contígua selecionada para o buffer oposto, preservando a gravação explícita.", "pub fn copy_difference"),
    ("CopySideAction", "parcial", "Barra Copiar e setas copiam seção conforme lado ativo; sem todas as opções de seleção do original.", "pub fn copy_difference"),
    ("SelectAllAction CutAction PasteAction", "presente", "Texto: Ctrl+A/Ctrl+X/Ctrl+V e menus; recortar/colar disponíveis em Editar, copiar também em leitura. Seleção segue o buffer real.", '"Recortar  Ctrl+X"'),
    ("DeleteAction DeleteWordBeginAction DeleteWordEndAction", "parcial", "Widget de texto oferece exclusão/atalhos de palavras durante edição; menus/contextos específicos do original ausentes.", "TextEdit::multiline(text)"),
    ("EndingStyleListAction ToWindowsAction ToUnixAction ToMacAction", "presente", "Seletor PC/Unix/Mac altera terminador de saída e estado dirty; misto preserva terminadores por linha. Conversão só grava ao salvar.", "document.line_ending"),
    ("FullEditAction", "presente", "F2 e Editar alternam mutabilidade sem remover cores ou alinhamento. Leitura permite seleção e cópia.", "Key::F2"),
    ("DiffSectionNextAction DiffSectionPrevAction", "presente", "Ctrl+N/Ctrl+P e barra navegam por seções contíguas. Botões desabilitam nas bordas.", "fn next_diff"),
    ("ShowContextAction", "parcial", "Mostra três linhas de contexto por lado; número de linhas não configurável.", "context_lines"),
    ("ShowItemNumAction", "presente", "Opção de números das linhas no menu Visualizar.", '"Números das linhas"'),
    ("ShowThumbnailAction", "parcial", "Miniatura com navegação por clique/arrasto sempre visível; ação de ocultar ausente.", "THUMBNAIL_ORPHAN"),
    ("ShowTextDetailsAction", "parcial", "Detalhes de texto da linha selecionada sempre presentes; sem todas as opções/configurações originais.", '"character_details"'),
]:
    mark("TTEXTACTIONS", names, state, note, TEXT, anchor)

for form, names, state, note, anchor in [
    ("TDIRBASEACTIONS", "BrowseListAction BrowseLeftAction BrowseRightAction", "presente", "Seletores de pastas locais nativos, com janela principal como proprietária.", "dialogs.folder"),
    ("TDIRBASEACTIONS", "StopAction", "presente", "Cancela a comparação em segundo plano; resultados cancelados são identificados como parciais.", '"Parar"'),
    ("TDIRBASEACTIONS", "SwapAction", "presente", "Inverte caminhos e inicia nova leitura, cancelando a anterior.", "std::mem::swap"),
    ("TDIRBASEACTIONS", "RefreshAction FullRefreshAction", "parcial", "F5/Ctrl+F5 e Atualizar fazem nova varredura completa; não há distinção otimizada ou atualização por seleção.", "fn start_scan"),
    ("TDIRBASEACTIONS", "OpenAction", "parcial", "Duplo clique abre somente pares de arquivos existentes; pastas/órfãos não têm navegação/abertura equivalentes.", "response.double_clicked()"),
    ("TDIRBASEACTIONS", "FindFilenameAction", "parcial", "Campo de filtro por substring; sem diálogo Ctrl+F, próximo/anterior ou sintaxe original.", "let query ="),
    ("TDIRBASEACTIONS", "ShowLogAction", "parcial", "Log da comparação exibido em painel fixo; não há alternância nem exportação.", '"folder_log"'),
    ("TDIRACTIONS", "NameFilterEditAction", "parcial", "Filtro simples por substring, não expressões *.*, exclusões, listas ou presets do original.", "let query ="),
    ("TDIRACTIONS", "CompareAction", "parcial", "Compara conteúdo recursivo ou tamanho/data e abre um par por duplo clique; sem comparação da seleção.", "fn scan_folders"),
    ("TDIRACTIONS", "FavCompareAllAction FavDiffAction FavSameAction", "presente", "Filtros mutuamente exclusivos para a lista básica. Estados de diretórios não são agregados aos descendentes.", '"tb_all"'),
    ("TDIRACTIONS", "DirCompareStructureAction DirFlattenAction", "parcial", "Lista recursiva de caminhos relativos; árvore, agregação, alternância e alinhamento de nomes ausentes.", "let visible: Vec<&Entry>"),
]:
    mark(form, names, state, note, FOLDER, anchor)

mark("THEXACTIONS", "DiffItemNextAction DiffItemPrevAction", "parcial", "Navegação por bytes diferentes e atalhos; granularidade e estados do original não foram integralmente verificados.", HEX, '"Próxima diferença  Ctrl+N"')
mark("THEXACTIONS", "ShowItemNumAction", "parcial", "Endereços em hexadecimal sempre visíveis; sem alternância/configuração.", HEX, '"{:08X}"')
mark("THOMEACTIONS", "FindAction", "parcial", "Filtro do histórico próprio implementado; atalho Ctrl+F e gestão completa de sessões ausentes.", HOME, "search_query")
mark("THOMEACTIONS", "ShowManageAction", "parcial", "Barra lateral própria sempre visível; não há alternância ou toda a gestão do original.", HOME, '"home_sessions"')
mark("THOMEACTIONS", "ShowWebAction", "excluida_usuario", "Faixa vermelha de suporte, tutoriais e registro removida por instrução explícita de 14/10/2026.", HOME, "CentralPanel::default()")

# Reviewed implementations added since the first inventory. These links describe
# concrete local behavior; they do not turn a disabled label into a feature.
def revise(form, names, note, path, anchor, state="parcial"):
    for name in names.split():
        assessments[(form, name)] = {"status": state, "assessment": note, "source": location(path, anchor)}

SESSION_MENU = "src/ui/session_menu.rs"
revise("TBASEACTIONS", "WindowNewAction", "Abre nova instância do executável próprio.", SESSION_MENU, "Request::NewWindow")
revise("TBASEACTIONS", "SessionOpenAction WorkspaceLoadListAction WorkspaceSaveAsAction", "Sessões e espaços de trabalho JSON locais preservam caminhos/opções e confirmam alterações antes de substituir sessões. Sem importação do formato BC4 nem gerenciador completo.", "src/workspace.rs", "pub fn load")
revise("TBASEACTIONS", "ProgramOptionsAction", "Opções locais incluem escolha de idioma automático/pt-BR/inglês; conjunto completo do original não reproduzido.", "src/i18n.rs", "pub fn language_field")
revise("TBASEACTIONS", "ExportAction ImportAction RestoreAction", "Exporta/importa/restaura configurações próprias; valida entrada, preserva backup e não manipula o perfil BC4.", "src/i18n.rs", "pub fn settings_file")
revise("TSESSIONACTIONS", "SessionSaveAction SessionSaveAsAction", "Grava caminhos e opções da sessão em JSON próprio; alterações dos documentos continuam exigindo Salvar arquivo.", MAIN, "session_paths")
revise("TSESSIONACTIONS", "SessionEditAction RulesAction", "Configuração nativa com regras básicas de caixa/espaços; gramáticas e regras avançadas ausentes.", TEXT, "fn rules_dialog")
revise("TSESSIONACTIONS", "ReportAction", "Relatórios HTML/TXT de texto/pastas; seleção/opções disponíveis não abrangem os layouts originais.", "src/report.rs", "pub fn export")
revise("TSESSIONACTIONS", "InfoAction", "Informações da comparação de texto em diálogo nativo.", TEXT, "fn comparison_info")
revise("TSESSIONACTIONS", "InsigIgnoredAction", "Ignora diferenças de caixa/espaços por opção; não todas as regras de importância do BC4.", TEXT, "fn active_rules")
revise("TFILEACTIONS", "SaveAction SaveAsAction SaveAsLocalAction", "Texto e binário editáveis com gravação explícita e conflito externo; texto preserva BOM/encoding/EOL, rejeita perda em ANSI e oferece codificação Unicode.", TEXT, "pub fn save_side")
revise("TFILEACTIONS", "CopyAction SelectAllAction", "Seleção/cópia em texto editável/leitura e no hexadecimal; contextos avançados não verificados.", TEXT, '"Selecionar tudo  Ctrl+A"')
revise("TFILEACTIONS", "FindAction FindNextAction FindPrevAction", "Texto: Unicode, regex, caixa, palavra inteira e ambos os lados; hexadecimal: procura bytes/UTF-8.", TEXT, "fn find")
revise("TFILEACTIONS", "ShowWSAction", "Alterna marcadores visuais de espaços/tabulações no texto sem mudar os buffers.", TEXT, "show_whitespace")
revise("TTEXTACTIONS", "ReplaceAction", "Substitui ocorrência/todas com undo, regex e seleção de lados no texto.", TEXT, "fn replace")
revise("TTEXTACTIONS", "GoToAction", "Prompt nativo linha:coluna com revelação de linhas filtradas.", TEXT, "fn go_to")
revise("TTEXTACTIONS", "CopyItemRightAction CopyItemLeftAction", "Copia linha alinhada para o outro buffer; compatibilidade com todas as seleções do original não verificada.", TEXT, "fn copy_line")
revise("TTEXTACTIONS", "IncreaseIndentAction DecreaseIndentAction InsertItemBeforeAction InsertItemAfterAction DeleteItemAction", "Operações de linhas reais com desfazer; sem todos os modos e seleções do original.", TEXT, "fn edit_lines")
revise("TTEXTACTIONS", "TrimTrailWSAction LeadSpaceToTabAction TabToSpaceAction", "Conversões de espaços/tabs no buffer com desfazer e gravação posterior explícita.", TEXT, "fn transform_active")
revise("TTEXTACTIONS", "ShowThumbnailAction ShowTextDetailsAction", "Alterna miniatura e detalhes; configuração completa dos painéis do original não reproduzida.", TEXT, "show_details")
revise("TDIRBASEACTIONS", "FindFilenameAction FindFilenameNextAction FindFilenamePrevAction", "Localiza próximo/anterior nome por substring com prompt nativo; sem sintaxe completa de máscaras BC4.", FOLDER, "fn navigate")
revise("TDIRBASEACTIONS", "ShowLogAction", "Painel de log alternável; exportação e todos os controles originais ausentes.", FOLDER, "show_log")
revise("TDIRBASEACTIONS", "ExpandAction CollapseAction ExpandAllAction CollapseAllAction", "Árvore de pastas com expansão/recolhimento; todos os contextos e seleções BC4 não verificados.", FOLDER, "collapsed")
revise("TDIRACTIONS", "DirCompareStructureAction DirFlattenAction FavCompareAllAction FavDiffAction FavSameAction", "Árvore/lista local com estados agregados e filtros; alinhamento manual de nomes e filtros avançados ausentes.", FOLDER, "collapsed")
revise("TDIRACTIONS", "CopyRightAction CopyLeftAction CopyAction CopySideAction", "Cópia local em segundo plano com prévia de sobrescrita, cancelamento e gravação temporária; ACLs/atributos completos e todos os perfis não reproduzidos.", FOLDER, "fn copy_selected")
revise("TDIRACTIONS", "DeleteAction", "Exclusão confirmada para Lixeira do Windows; exclusão permanente não implementada.", FOLDER, "fn delete_selected")
revise("TDIRACTIONS", "RenameAction", "Renomeia item local selecionado com validação do nome.", FOLDER, "fn rename_selected")
revise("TDIRACTIONS", "NewFolderAction", "Cria pasta local no lado ativo com validação do nome.", FOLDER, "fn new_folder")
revise("THEXACTIONS", "GoToAction", "Ir para offset com prompt nativo e validação.", HEX, "fn go_to")
revise("THEXACTIONS", "CopyRightAction CopyLeftAction CopySideAction", "Copia intervalo selecionado entre documentos binários; não todos os modos de seleção BC4.", HEX, "fn copy_side")
revise("THEXACTIONS", "CutAction PasteAction DeleteAction", "Edição binária, clipboard e undo; edição/atalhos disponíveis não equivalem a todos os comandos de menu originais.", HEX, "fn change")
revise("THEXACTIONS", "ShowItemNumAction LayoutAction LittleEndianListAction BigEndianListAction", "Opções de layout/endereço e endian no hexadecimal próprio; equivalência visual completa não verificada.", HEX, "fn options")

data = json.loads(REFERENCE.read_text(encoding="utf-8"))
keys = {(row["form"], row["name"]) for row in data["actions"]}
unknown = assessments.keys() - keys
if unknown:
    raise ValueError(f"Assessment without recovered declaration: {unknown}")


def missing_note(form):
    if form == "TDIRACTIONS" or form == "TDIRBASEACTIONS":
        return "Não há implementação deste comando de pastas; árvore/seleções/operações de arquivos ainda incompletas."
    if form == "TDIRSYNCACTIONS":
        return "Sincronização de pastas ainda não implementada; botão permanece desabilitado."
    if form in {"TDATAACTIONS", "TREGACTIONS", "TPIXACTIONS", "TMP3ACTIONS", "TVERACTIONS"}:
        return "Comparador correspondente ainda não implementado."
    if form == "THEXACTIONS":
        return "Comando hexadecimal sem vínculo de implementação nesta auditoria; o módulo já permite edição e salvamento."
    return "Não foi localizada implementação deste comando no código atual. Declaração recuperada não prova comportamento."


actions = []
for row in data["actions"]:
    assessment = assessments.get((row["form"], row["name"]))
    if assessment is None:
        other_platform = row["name"].startswith("Mac") or row["name"] == "RevealInFinderAction"
        assessment = {"status": "fora_windows" if other_platform else "ausente",
                      "assessment": "Comando específico de macOS fora do alvo Windows." if other_platform else missing_note(row["form"])}
    actions.append({**row, **assessment})

partial_forms = {
    "TABOUTDIALOG": (DIALOGS, "fn about(&mut self) {", "Mensagem nativa própria, não todos os componentes do original."),
    "TBCDIALOG": (DIALOGS, "fn message(", "Padrão modal nativo Windows aplicado às caixas existentes. Formulários complexos futuros ainda faltam."),
    "TFILECONFIRMSAVEDIALOG": (DIALOGS, "fn confirm_save(&mut self, left: bool, path:", "Confirmação por arquivo Sim/Não/Cancelar. Layout alternativo de múltiplos arquivos/checkboxes não reproduzido."),
    "TTABSCONFIRMCLOSEDIALOG": (MAIN, "if self.pending_exit {", "Fluxo seguro de fechar com confirmação; opção fechar todas/sempre fechar todas ausente."),
    "THOMEVIEWFRAME": (HOME, "pub fn ui(", "Home e histórico próprios; histórico/sessões/workspaces próprios; sem gestão completa e compatibilidade com os formatos BC4."),
    "TTEXTVIEWFRAME": (TEXT, "pub fn ui(", "Dois lados, edição, faixas, pesquisa/substituição e regras básicas; merge e opções avançadas ausentes."),
    "TDIRBASEVIEWFRAME": (FOLDER, "pub fn ui(", "Árvore/lista local com operações e relatórios; contextos avançados e perfis ausentes."),
    "TDIRVIEWFRAME": (FOLDER, "pub fn ui(", "Árvore/lista local com operações e relatórios; sincronização/merge e filtros avançados ausentes."),
    "THEXVIEWFRAME": (HEX, "pub fn ui(", "Grade editável, seleção, undo, salvar, busca/offset e layout; diversos comandos e detalhes visuais ainda divergem."),
    "TFILECODEPAGEERRORDIALOG": (TEXT, "fields_with_accept", "Aviso nativo de perda em ANSI com escolha Unicode e Salvar/Cancelar; nem todas as codepages do original suportadas."),
    "TFILEVIEWFRAME": (TEXT, "pub fn ui(", "Cabeçalhos e barra parciais; herança original não reproduzida integralmente."),
    "TSESSIONVIEWFRAME": (MAIN, "enum Session {", "Sessões próprias em abas; sem gestão/configurações originais completas."),
    "TSESSIONFRAME": (MAIN, "enum Session {", "Modelo próprio de sessões; sem todos os tipos/configurações."),
    "TVIEWFORM": (MAIN, "fn main()", "Janela Windows nativa com conteúdo egui; renderização, menus e fontes ainda não são equivalência pixel a pixel."),
    "TVIEWFRAME": (MAIN, '"session_tabs"', "Estrutura de abas própria; opções de janelas/abas do original incompletas."),
    "TLOGFRAME": (FOLDER, '"folder_log"', "Log de pastas próprio sem todos os controles/ações."),
    "TWEBBROWSERFRAME": (HOME, "CentralPanel::default()", "Faixa web da Home excluída a pedido; navegador de outras funções não implementado."),
}
forms = []
for row in data["forms"]:
    form = dict(row)
    if row["name"].endswith("ACTIONS") or row["name"].startswith("TBCIMAGES"):
        form.update(status="recurso", assessment="Módulo de declarações/imagens, não uma janela independente. Cobertura de ações avaliada separadamente.")
    elif row["name"] in partial_forms:
        path, anchor, note = partial_forms[row["name"]]
        form.update(status="parcial", assessment=note, source=location(path, anchor))
    else:
        form.update(status="ausente", assessment="Formulário/layout específico ainda não reproduzido.")
    forms.append(form)

# Call sites are a source-navigation aid: dynamic arrays and composite controls can
# emit several buttons, so this must never be advertised as an exact button count.
controls = []
pattern = re.compile(r"\b(?:ui|column)\.(?:button|small_button|menu_button|checkbox|radio_value|selectable_label|add_enabled)\s*\(|\b(?:tool|big_session_button|session_link|menu_item|menu_button)\s*\(")
for path, source in sources.items():
    if path == "src/ui/icon_catalog.rs":
        continue
    source = source.split("#[cfg(test)]\nmod tests")[0]
    for match in pattern.finditer(source):
        controls.append({"path": path, "line": source.count("\n", 0, match.start()) + 1,
                         "declaration": re.sub(r"\s+", " ", source[match.start():match.start()+260]).strip()})

assert not any("Window::new(" in s for p, s in sources.items() if "/ui/" in p), "Unreviewed egui floating dialog"
assert not any("rfd::" in s for p, s in sources.items() if p != DIALOGS), "Dialog bypasses shared host"
counts = {"actions": dict(Counter(r["status"] for r in actions)),
          "forms": dict(Counter(r["status"] for r in forms)),
          "control_declaration_sites": len(controls)}
report = {
    "date": str(date.today()), "version": "0.1.5", "method": "Revisão estática autoral do código + inventário de recursos autenticado pelo REA; testes de execução documentados separadamente.",
    "reference_evidence_id": data["source_evidence_id"], "reference_artifact": data["artifact"],
    "reference_inventory_sha256": hashlib.sha256(REFERENCE.read_bytes()).hexdigest(),
    "source_sha256": {p: hashlib.sha256((ROOT / p).read_bytes()).hexdigest() for p in sources if p != "src/ui/icon_catalog.rs"},
    "labels": LABELS, "counts": counts,
    "limitations": ["501 declarações incluem herança/sobrescritas e outros sistemas; não são 501 funções independentes.",
                    "200 recursos de formulários incluem frames, módulos de ações e imagens; não são 200 caixas de diálogo independentes.",
                    "Presente significa disponível no escopo descrito; não significa equivalência comprovada ao original.",
                    "Classificação conservadora: itens sem vínculo concreto são ausentes; a identificação manual pode ser refinada com nova evidência.",
                    "Nenhuma alegação de teste de todas as ações do original; screenshots e testes próprios cobrem somente os casos documentados.",
                    "Catálogo de controles lista pontos de declaração, não quantidade exata de botões nem prova de execução de cada um.",
                    "Diálogos existentes agora seguem Windows nativo; opções, formulários complexos e módulos ausentes ainda precisam ser implementados."],
    "actions": actions, "forms": forms, "control_declarations": controls,
}
OUT.mkdir(parents=True, exist_ok=True)
(OUT / "coverage.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
payload = json.dumps(report, ensure_ascii=False).replace("<", "\\u003c")
html = r'''<!doctype html><html lang="pt-BR"><meta charset="utf-8"><title>AnyCompare — revisão 0.1.5</title>
<style>body{font:14px Segoe UI,Arial;margin:28px;color:#17212c;background:#f7f8fa}h1{font-size:24px}p{max-width:1100px;line-height:1.5}input,select{font:inherit;padding:9px;margin:0 8px 12px 0}input{width:420px}table{border-collapse:collapse;width:100%;background:white}td,th{text-align:left;vertical-align:top;padding:10px;border-bottom:1px solid #dde2e8}th{position:sticky;top:0;background:#e3e7ed}code{font-size:12px;overflow-wrap:anywhere}.presente{color:#12622c}.ausente{color:#a02424}.parcial{color:#855800}small{color:#536477}.cards{display:flex;gap:12px;flex-wrap:wrap;margin:15px 0}.cards div{background:white;padding:14px;border:1px solid #d9dfe5}#total{margin:8px 0}</style>
<h1>Revisão de botões, visual e funções — AnyCompare 0.1.5</h1>
<p>Inventário completo das <b>501 declarações de ações</b> e dos <b>200 recursos de formulários</b> recuperados do original, confrontado com o código atual. Inclui heranças, frames e módulos; os números não equivalem a funções ou janelas independentes. “Presente” se refere ao escopo descrito, sem comprovar equivalência integral ao BC4.</p>
<div class="cards" id="cards"></div><p><b>Correções desta versão:</b> salvar somente quando alterado; confirmação nativa Sim/Não/Cancelar ao abrir/recarregar/fechar; erros e Sobre no mesmo padrão nativo; refazer; atalhos; seleção/cópia em leitura; ícones desabilitados em cinza; proteção contra resultados antigos de comparação de pastas. A faixa vermelha continua removida por instrução do usuário.</p>
<p><b>Pendências principais:</b> regras avançadas, sincronização, mesclagem de três vias, comparadores adicionais, perfis remotos e formatos BC4 de sessões/configurações. Texto, pastas e hexadecimal têm comandos locais implementados; equivalência visual e funcional completa ainda não foi demonstrada. A validação de execução está em docs/validation; este inventário é uma revisão estática autoral.</p>
<input id="query" placeholder="Pesquisar nome, atalho, módulo, observação ou código"><select id="kind"><option value="actions">Ações do original</option><option value="forms">Formulários / frames / módulos</option><option value="control_declarations">Declarações de controles atuais</option></select><select id="state"><option value="">Todos os estados</option></select><div id="total"></div><table><thead id="head"></thead><tbody id="body"></tbody></table>
<script>const data=__DATA__;const $=id=>document.getElementById(id);for(const [k,v]of Object.entries(data.labels)){let o=document.createElement('option');o.value=k;o.textContent=v;$('state').append(o)}for(const[k,n]of Object.entries(data.counts.actions)){let el=document.createElement('div');el.className=k;el.textContent=n+' — '+data.labels[k];$('cards').append(el)}function cell(tr,value,code=false){let td=document.createElement('td');if(code){let c=document.createElement('code');c.textContent=value;td.append(c)}else td.textContent=value;tr.append(td)}function draw(){let kind=$('kind').value,q=$('query').value.toLocaleLowerCase(),s=$('state').value,rows=data[kind].filter(r=>(!s||r.status===s)&&JSON.stringify(r).toLocaleLowerCase().includes(q));$('total').textContent=rows.length+' de '+data[kind].length+' registros';$('head').textContent='';let hr=document.createElement('tr');for(let t of(kind==='control_declarations'?['Arquivo / linha','Declaração (trecho)']:['Módulo / nome','Rótulo / atalho','Estado','Avaliação / evidência no código'])){let th=document.createElement('th');th.textContent=t;hr.append(th)}$('head').append(hr);$('body').textContent='';for(let r of rows){let tr=document.createElement('tr');if(kind==='control_declarations'){cell(tr,r.path+':'+r.line,true);cell(tr,r.declaration,true)}else{cell(tr,(r.form?r.form+' / ':'')+r.name,true);cell(tr,r.props?(r.props.Caption||'')+' '+(r.props.ShortCutStr||''):(r.components+' componentes · SHA '+r.sha256));cell(tr,data.labels[r.status]);tr.lastChild.className=r.status;cell(tr,r.assessment+(r.source?' · '+r.source.path+':'+r.source.line:''))}$('body').append(tr)}}for(let el of[$('query'),$('kind'),$('state')])el.addEventListener('input',draw);draw();</script></html>'''
(OUT / "index.html").write_text(html.replace("__DATA__", payload), encoding="utf-8")
print(json.dumps(counts, ensure_ascii=False))
