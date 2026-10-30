"""Inventory of BC4 commands per comparison type, crossed with AnyCompare status.

Inputs (all already evidence-backed):
- docs/rea/forms.json            TUiAction declarations per action module (DFM, CONFIRMADO ESTATICAMENTE)
- docs/rea/menus-composed.json   effective trees after inherited merge + prune (tools/compose_rea_menus.py)
- docs/review/coverage.json      AnyCompare 0.1.5 static review per action (presente/parcial/ausente/...)

Outputs: docs/rea/inventory-functions.json and docs/rea/INVENTARIO-FUNCOES.md.
"""

import collections
import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REA = ROOT / "docs" / "rea"
REVIEW = ROOT / "docs" / "review"

# Session types created by TBaseActions VMT+0x98 (0x1944900) with handlers
# DirCompareNewExecute..XMLCompareNewExecute (0x1942410..0x1942650).
# Module per type is INFERIDO from names (unknown unk_8535d836...).
SESSION_TYPES = [
    ("DirCompare", "Comparação de pastas", "TDirActions"),
    ("DirMerge", "Mesclagem de pastas", "TDirActions"),
    ("DirSync", "Sincronização de pastas", "TDirSyncActions"),
    ("TextCompare", "Comparação de texto", "TTextActions"),
    ("TextMerge", "Mesclagem de texto", "TTextActions"),
    ("TableCompare", "Comparação de tabelas", "TDataActions"),
    ("DataCompare", "Comparação de dados (CSV/planilhas)", "TDataActions"),
    ("HexCompare", "Comparação hexadecimal (binária)", "THexActions"),
    ("MP3Compare", "Comparação de MP3", "TMP3Actions"),
    ("PixCompare", "Comparação de imagens", "TPixActions"),
    ("RegCompare", "Comparação de registro", "TRegActions"),
    ("VerCompare", "Comparação de versão (PE)", "TVerActions"),
    ("XMLCompare", "Comparação de XML", "TTextActions"),
]

MODULE_TITLES = {
    "TBaseActions": "Comuns a todas as janelas (Base)",
    "THomeActions": "Home",
    "TSessionActions": "Comuns às sessões",
    "TFileActions": "Comuns às comparações de arquivo",
    "TDirBaseActions": "Comuns a pastas (DirBase)",
    "TDirActions": "Comparação/mesclagem de pastas",
    "TDirSyncActions": "Sincronização de pastas",
    "TTextActions": "Texto (comparação, mesclagem, XML)",
    "THexActions": "Hexadecimal / binário",
    "TDataActions": "Tabela / dados",
    "TPixActions": "Imagem",
    "TMP3Actions": "MP3",
    "TRegActions": "Registro",
    "TVerActions": "Versão",
}

# Option/dialog forms per type, selected by DFM resource-name prefix.
FORM_PREFIXES = {
    "TTextActions": ("TTEXT",),
    "THexActions": ("THEX",),
    "TDataActions": ("TDATA",),
    "TPixActions": ("TPIX",),
    "TMP3Actions": ("TMP3",),
    # Not "TREG": TREGISTERDIALOG is the license dialog (out of scope).
    "TRegActions": ("TREGBROWSE", "TREGFIND", "TREGREPORT", "TREGVAL", "TREGVIEW", "TBROWSEREGISTRY"),
    "TVerActions": ("TVER",),
    "TDirActions": ("TDIR", "TOP"),
    "TDirSyncActions": ("TDIRSYNC", "TOPSYNC"),
    "TFileActions": ("TFILE",),
}

STATUS_ORDER = ["presente", "parcial", "ausente", "fora_windows", "excluida_usuario"]

# Corrections to docs/review/coverage.json found after that review (kept separate on purpose).
_COPY_SELECTION = (
    "Correção 16/10/2026: copia as linhas selecionadas no painel ativo ou, sem seleção, a seção atual "
    "(copy_selected → selected_rows + DiffEngine::merge_range, src/ui/text_compare.rs:395) na barra Copiar, "
    "em Ctrl+L/Ctrl+R e no menu Editar; o botão fica habilitado com seleção mesmo em linha igual. Linha "
    "parcialmente selecionada conta inteira; seleção que termina logo após a quebra de linha não inclui a "
    "linha seguinte; selecionar o documento inteiro (Ctrl+A) cobre também as linhas que só existem no outro "
    "lado antes da primeira e depois da última linha. Testes copy_* em text_compare.rs (Ctrl+A + Copiar para "
    "a Esquerda, seleção parcial, sem seleção, CRLF/BOM). Limites: a regra das lacunas nas bordas é inferida, "
    "não comparada em execução com o original; o popup da margem (Gutter) não existe no AnyCompare. "
    "Margem (17/10/2026): faixa de 35 px clicável inteira (medida no original a 100%); sem seleção, seta "
    "amarela 'Copiar Seção para …' por diferença; com seleção, uma única seta azul 'Copiar Linha(s) para …' "
    "na primeira linha selecionada, que copia só as linhas selecionadas (dicas do BCompare.tr; teste "
    "gutter_arrow_copies_only_the_selected_line_or_the_whole_section)."
)
OVERRIDES = {
    ("TTEXTACTIONS", "CopyRightAction"): ("presente", _COPY_SELECTION),
    ("TTEXTACTIONS", "CopyLeftAction"): ("presente", _COPY_SELECTION),
    ("TTEXTACTIONS", "CopySideAction"): ("presente", _COPY_SELECTION),
}


def sha(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


def placements(composed):
    """action name -> list of 'Module:/Top:Kind/...' paths in the effective trees."""
    out = collections.defaultdict(set)
    for m in composed["modules"]:
        def walk(nodes, path):
            for n in nodes:
                here = f"{path}/{n['name']}"
                if n.get("action"):
                    top = here.split("/")[1]
                    kind = next((t["type"] for t in m["windows_top_level"] if t["name"] == top), "?")
                    out[(m["module"], n["action"].lower())].add(f"{kind}:{here.lstrip('/')}")
                walk(n["children"], here)
        walk(m["nodes"], "")
    return out


def main():
    forms = json.loads((REA / "forms.json").read_text(encoding="utf-8"))
    composed = json.loads((REA / "menus-composed.json").read_text(encoding="utf-8"))
    coverage = json.loads((REVIEW / "coverage.json").read_text(encoding="utf-8"))
    status = {(a["form"], a["name"]): a for a in coverage["actions"]}
    place = placements(composed)

    modules = []
    for f in forms:
        h = f["hierarchy"]
        cls = h["class"]
        if cls not in MODULE_TITLES:
            continue
        actions = []

        def walk(n):
            if n.get("class") == "TUiAction":
                p = n.get("props", {})
                cov = dict(status.get((f["name"], n["name"]), {}))
                override = OVERRIDES.get((f["name"], n["name"]))
                if override:
                    cov["status"], cov["assessment"] = override
                actions.append({
                    "name": n["name"],
                    "category": p.get("Category"),
                    "caption": p.get("Caption"),
                    "hint": (p.get("Hint") or "").split("|")[-1] or None,
                    "shortcut": p.get("ShortCutStr"),
                    "placements": sorted(place.get((cls, n["name"].lower()), [])),
                    "anycompare_status": cov.get("status"),
                    "anycompare_assessment": cov.get("assessment"),
                    "override": bool(override),
                })
            for c in n.get("children", []):
                walk(c)

        walk(h)
        prefixes = FORM_PREFIXES.get(cls, ())
        option_forms = sorted(
            x["name"] for x in forms
            if prefixes and x["name"].startswith(prefixes) and not x["name"].endswith("ACTIONS")
        )
        comp = next((m for m in composed["modules"] if m["module"] == cls), None)
        modules.append({
            "module": cls,
            "title": MODULE_TITLES[cls],
            "resource": f["name"],
            "load_order": comp["load_order"] if comp else [cls],
            "windows_menus": [t["name"] for t in comp["windows_top_level"] if t["type"] == "Menu"] if comp else [],
            "windows_toolbars": [t["name"] for t in comp["windows_top_level"] if t["type"] == "Toolbar"] if comp else [],
            "popups": [t["name"] for t in comp["windows_top_level"] if t["type"] == "PopupMenu"] if comp else [],
            "status_counts": dict(collections.Counter(a["anycompare_status"] for a in actions)),
            "option_forms": option_forms,
            "actions": actions,
        })

    order = list(MODULE_TITLES)
    modules.sort(key=lambda m: order.index(m["module"]))
    doc = {
        "target_sha256": "aba1b98f848a01c3729cfee3e8818ab8e0d947276bc42c9a55c1475c9f8438db",
        "inputs": {
            "docs/rea/forms.json": sha(REA / "forms.json"),
            "docs/rea/menus-composed.json": sha(REA / "menus-composed.json"),
            "docs/review/coverage.json": sha(REVIEW / "coverage.json"),
        },
        "anycompare_review_version": coverage.get("version"),
        "session_types": [{"id": t, "title": d, "module_inferido": m} for t, d, m in SESSION_TYPES],
        "modules": modules,
    }
    (REA / "inventory-functions.json").write_text(json.dumps(doc, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    (REA / "INVENTARIO-FUNCOES.md").write_text(render_md(doc), encoding="utf-8")
    total = collections.Counter()
    for m in modules:
        total.update(m["status_counts"])
        print(f"{m['module']:16} {len(m['actions']):4} {m['status_counts']}")
    print("total", sum(total.values()), dict(total))
    return 0


def esc(s):
    return (s or "").replace("|", "\\|").replace("&", "")


def render_md(doc):
    L = []
    L.append("# Inventário de funções do Beyond Compare 4.4.7 por tipo de comparação")
    L.append("")
    L.append("Gerado por `tools/build_function_inventory.py` a partir de `docs/rea/forms.json` (declarações "
             "`TUiAction` dos DFM, **CONFIRMADO ESTATICAMENTE**), `docs/rea/menus-composed.json` (árvores "
             "efetivas após herança e poda, ver `docs/rea/menus-composed.json` → `rule`) e "
             f"`docs/review/coverage.json` (revisão estática do AnyCompare {doc['anycompare_review_version']}). "
             "Os status do AnyCompare vêm da revisão manual: presença no fonte não é equivalência funcional. "
             "Correções posteriores à revisão estão em `OVERRIDES` no gerador e aparecem com o texto "
             "\"Correção 16/10/2026\".")
    L.append("")
    L.append("## Tipos de sessão")
    L.append("")
    L.append("13 tipos criados dinamicamente por `TBaseActions` VMT+0x98 (0x1944900), com handlers "
             "`DirCompareNewExecute`…`XMLCompareNewExecute` (0x1942410–0x1942650). O módulo de ações de cada "
             "tipo é **INFERIDO** pelos nomes (lacuna `unk_8535d836…`).")
    L.append("")
    L.append("| Tipo | Descrição | Módulo de ações (INFERIDO) |")
    L.append("|---|---|---|")
    for t in doc["session_types"]:
        L.append(f"| {t['id']} | {t['title']} | {t['module_inferido']} |")
    L.append("")
    L.append("## Resumo por módulo")
    L.append("")
    L.append("| Módulo | Escopo | Ações próprias | Presente | Parcial | Ausente | Fora do Windows | Menus no Windows |")
    L.append("|---|---|---|---|---|---|---|---|")
    for m in doc["modules"]:
        c = m["status_counts"]
        L.append(f"| {m['module']} | {m['title']} | {len(m['actions'])} | {c.get('presente', 0)} | "
                 f"{c.get('parcial', 0)} | {c.get('ausente', 0)} | {c.get('fora_windows', 0)} | "
                 f"{', '.join(m['windows_menus'])} |")
    L.append("")
    L.append("Cada módulo herda as ações dos ancestrais (`load_order`); as tabelas abaixo listam só as ações "
             "declaradas no próprio módulo. Os menus vêm da fusão herdada com poda: `Mac`, `MacTools`, "
             "`MacWindow` e `MergeFile` saem no Windows; Base/Home não têm barra de ferramentas.")
    for m in doc["modules"]:
        L.append("")
        L.append(f"## {m['title']} — `{m['module']}`")
        L.append("")
        L.append(f"Herança: {' → '.join(m['load_order'])}. Menus (Windows): {', '.join(m['windows_menus']) or '—'}. "
                 f"Barras: {', '.join(m['windows_toolbars']) or '—'}. Popups: {', '.join(m['popups']) or '—'}.")
        if m["option_forms"]:
            L.append("")
            L.append(f"Formulários de opções/diálogos do tipo ({len(m['option_forms'])}): "
                     + ", ".join(f"`{x}`" for x in m["option_forms"]) + ".")
        L.append("")
        L.append("| Ação | Legenda (EN) | Atalho | Categoria | O que faz | Onde aparece | AnyCompare |")
        L.append("|---|---|---|---|---|---|---|")
        for a in m["actions"]:
            where = "; ".join(p.split(":", 1)[0] + " " + p.split(":", 1)[1].split("/")[0] for p in a["placements"][:3])
            st = a["anycompare_status"] or "?"
            if a["anycompare_assessment"]:
                st += " — " + esc(a["anycompare_assessment"])
            L.append(f"| `{a['name']}` | {esc(a['caption'])} | {a['shortcut'] or ''} | {a['category'] or ''} | "
                     f"{esc(a['hint'])} | {esc(where)} | {st} |")
    L.append("")
    return "\n".join(L)


if __name__ == "__main__":
    sys.exit(main())
