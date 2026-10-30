# Guia — a melhor forma de clonar o Beyond Compare 4 com o REA

Pesquisa de 16/10/2026. Alvo: `C:\Projetos\BC4\bc4\BCompare.exe` 4.4.7.28397,
SHA-256 `aba1b98f848a01c3729cfee3e8818ab8e0d947276bc42c9a55c1475c9f8438db`.
Ambiente: REA 6.5.0 (npm), Ghidra 12.1.4 em `C:\ghidra`, JDK 21.0.10, Windows 11.

Rótulos usados: **CONFIRMADO ESTATICAMENTE**, **CONFIRMADO EM EXECUÇÃO**, **INFERIDO**,
**DESCONHECIDO**, **BLOQUEADO** (mesmas definições da Operação REA).

## 1. Resposta curta

O REA não "clona" um aplicativo: ele produz **evidência** sobre o original. O clone sai
de uma **especificação** extraída do original e é aceito por **testes diferenciais contra
o próprio original**. Para o BC4, a forma mais eficaz é:

1. **Especificação primeiro.** Menus, barras, popups, atalhos, ícones e textos do BC4 são
   *dados* dentro do executável (14 `TUiActionTree` + 501 `TUiAction`). Eles já foram
   extraídos em `docs/rea/menus.json` e `docs/rea/forms.json`.
2. **Código gerado, não escrito à mão.** O AnyCompare deve gerar seus menus a partir dessa
   especificação, em vez de declarar cada item em cada tela.
3. **Oráculo diferencial.** O próprio BC4 gera relatórios por script (texto, pastas em XML,
   hex, tabelas, imagens, MP3, registro, versão). Esses relatórios viram os resultados
   esperados dos testes.
4. **Execução observada.** O que depende do estado (habilitado, marcado, fusão de menus por
   contexto, geometria) é medido no original com Frida via REA.
5. **Ghidra só nas lacunas.** Decompile apenas o que não está nos recursos, na ajuda nem
   nos relatórios, como algoritmos e regras. Cada lacuna deve ser registrada antes com `record_unknown`.

É o mesmo ciclo do estudo de caso oficial (DX-Ball): uma função ou comando por vez,
constantes lidas do binário, comparação contra o original em casos representativos
e de borda, e progresso registrado num livro-razão.

## 2. Por que está difícil hoje (diagnóstico)

| # | Problema | Evidência | Efeito |
|---|----------|-----------|--------|
| D1 | Menus escritos à mão, tela por tela | 141 chamadas `menu_item`/`menu_button` com textos literais em `text_compare.rs` (50), `hex_compare.rs` (31), `folder_compare.rs` (30), `session_menu.rs` (16), `home.rs` (12) e `chrome.rs` (2) | Divergência inevitável: o original tem **1.506 nós** de menus, barras e popups |
| D2 | Escopo "1:1 total" sem fatias nem critério de pronto | Auditoria estática anterior (`docs/rea/manifest.json`): 26 ações presentes, 101 parciais, 359 ausentes, 14 fora do Windows, 1 excluída | Trabalho espalhado; nenhuma visão chega a ficar completa |
| D3 | REA do Claude Code sem Ghidra | Registro em `~/.claude.json` sem `env`; o `GHIDRA_INSTALL_DIR` só existia no Codex | Análise nativa **BLOQUEADA** nesta sessão. **Corrigido**; falta reconectar |
| D4 | Ferramentas pedidas que não existem no Windows | `binary_session`: Ghidra P0 tem 26 operações somente leitura | Ver seção 3; o roteiro precisa usar o que existe |
| D5 | Cada servidor MCP novo refaz a análise do Ghidra | Projetos Ghidra efêmeros; `BCompare.exe` tem 42 MB; timeout de 330 s na primeira tentativa | Agrupar perguntas por sessão longa e exportar evidências |
| D6 | Gerador do dossiê quebrado | `docs/rea/build.log`: `TypeError` em `tools/build_rea_dossier.py:265` | JSONs de `docs/rea` vêm da execução das 17:37; documentos 01–12 inexistentes |
| D7 | Sem rede de segurança | Repositório git **sem nenhum commit**; `dist/AnyCompare.exe` ainda é a 0.1.4 | Refatorações grandes (menus gerados) ficam arriscadas |

## 3. O que o REA faz e não faz neste Windows

Medido na sessão real (`binary_session`) e conferido na documentação `windows-ghidra-p0.md`.

| Disponível | Uso no clone |
|------------|--------------|
| `inspect_pe_resources` | Inventário autenticado (hash e intervalo de cada recurso) |
| Ghidra P0: `search_strings`, `list_strings`, `xrefs`, `search_bytes`, `read_bytes`, `analyze_function`, `procedure_pseudo_code`, `procedure_assembly`, `procedure_callers/callees`, `procedure_references`, `inspect_native_instruction`, `resolve_native_call_targets`, `address_to_file_offset`, `list_procedures`, `search_procedures` e outras (26 no total) | Lacunas algorítmicas: string → xref → dossiê → callees → constantes |
| `instrument_with_frida`, `start_frida_session` | Observar o original em execução (estado de ações, menus, janelas, fontes) |
| `record_unknown`, `update_unknown`, `list_unknowns`, `verify_unknown_resolution` | Registro formal das lacunas |
| `get_evidence_bundle`, `export_evidence_bundle`, `import_evidence_bundle`, `inspect_analysis_view` | Preservar e auditar evidências entre sessões |
| `build_reconstruction_obligation_ledger`, `evaluate_reconstruction_coverage`, `verify_reconstruction` | Contabilidade da reconstrução; só são úteis com evidências comparáveis (ver nota) |

| **Indisponível aqui** | Motivo |
|-----------------------|--------|
| `binary_overview`, `batch_decompile`, `trace_native_values`, `trace_native_ui_action`, `get_call_graph` | Fora das 26 operações do Ghidra P0 no Windows |
| `annotate_native_function`, `set_address_name(s)` | Windows P0 não altera a base de análise |
| `capture_process_scenario` | Exige PTY nativo (Linux/macOS) |
| `capture_native_ui_scenario`, `observe_native_ui`, `observe_native_calls` | Somente macOS |

**Nota sobre `verify_reconstruction`.** As afirmações aceitas são de três tipos:
comportamentais (capturas de processo), estruturais de função (`compare_functions`) e
estruturais de artefato. Para um clone em Rust/egui de uma GUI Delphi, comparar funções
não faz sentido, e a captura de processo não existe no Windows. Portanto a verificação
principal precisa ser **um arnês diferencial próprio** (seção 5). O REA fornece as evidências
do original, o registro de lacunas e os bundles.

`trace_native_ui_action` foi feito para ações de UI compiladas da Apple (nib/storyboard,
Objective-C) e não serve para o framework `TUi*` do BC4.

## 4. As fontes de verdade do original e a ferramenta para cada pergunta

| Pergunta | Fonte | Como extrair | Autoridade |
|----------|-------|--------------|------------|
| Estrutura de menus, barras, popups, atalhos, ícones, dicas e categorias | Recursos `TPF0` dos 14 módulos `T*Actions` (`TUiActionList` e `TUiActionTree.Nodes`) | `inspect_pe_resources` + decodificador próprio (já feito: `menus.json`, `forms.json`, `comparison.json`) | CONFIRMADO ESTATICAMENTE |
| Itens ocultos por padrão | Ajuda: asterisco (*) = "Hidden by default" | `hh.exe -decompile` de `BCompare.chm`, páginas `commands*.html` | Documentação oficial |
| O que cada comando faz | Ajuda: `commandsbc/home/text/textedit/textmerge/textpatch/dir/dirmerge/dirsync/hex/data/pix/mp3/reg/ver.html`, `session*.html`, `options*.html` | Mesmo processo | Especificação funcional (não prova de implementação) |
| Resultado funcional exato | Scripts do BC4: `text-report`, `folder-report layout:xml`, `hex-report`, `data-report`, `picture-report`, `mp3-report`, `registry-report`, `version-report`, `sync` | Cópia portátil controlada, perfil isolado, `BCompare.exe @script /silent` | CONFIRMADO EM EXECUÇÃO (oráculo) |
| Textos em PT-BR | `BCompare.tr` (externo; o REA recusou por entradas desalinhadas) | Analisador próprio, com hash registrado | CONFIRMADO ESTATICAMENTE após parser validado |
| Fusão dos menus por contexto, estados habilitado/marcado/visível, geometria e fontes | Processo em execução | Frida via `instrument_with_frida` (ver 5.3) | CONFIRMADO EM EXECUÇÃO |
| Ligação evento → código | RTTI/VMT x64 (`handlers.json`: 2.039 de 2.042 eventos ligados a endereços) | Já feito; ligação estática, não execução | CONFIRMADO ESTATICAMENTE |
| Algoritmos e regras não observáveis (alinhamento, pontuação, formatos, regras de sessão) | Código nativo | Ghidra via REA: `search_strings` → `xrefs` → `analyze_function` → `procedure_callees` → `read_bytes` | Pseudocódigo = observação de análise, não fonte original |

Ferramentas Delphi pesquisadas e por que não resolvem este alvo: o Pythia (NCC) só
lê binários de 32 bits. O Dhrake depende do IDR, que também é x86. O DelphiHelper (ESET)
suporta x64, mas exige IDA 8.4+ e só tem assinaturas IDR x64 de 2012 a 2014. Para o BC4 x64,
o caminho viável é o decodificador próprio de RTTI já iniciado, junto com Ghidra e Frida via REA.

## 5. Método recomendado

### 5.0 Ambiente (uma vez)

- REA no Claude Code com estas variáveis (aplicado em 16/10/2026; backup em
  `~/.claude/backups/claude.json.before-rea-ghidra-20261016-195352.bak`):
  `GHIDRA_INSTALL_DIR=C:\ghidra`, `JAVA_HOME=C:\Program Files\Eclipse Adoptium\jdk-21.0.10.7-hotspot`,
  `REA_GHIDRA_STARTUP_TIMEOUT_MS=2400000` e
  `REA_GHIDRA_DISABLED_ANALYZERS=Create Address Tables,Decompiler Switch Analysis`.
  Esse é o perfil focado das evidências anteriores. Para recuperar `switch`/`case` no
  pseudocódigo, remova a última variável e aceite uma análise mais longa.
- Reconectar o servidor `rea` (`/mcp`) e confirmar em `binary_session` que o Ghidra
  está `available`.
- Fazer um commit inicial no git antes de qualquer refatoração.

### 5.1 Congelar a especificação no repositório

Criar `spec/` com JSON versionado, gerado a partir das evidências e com hash e Evidence ID em cada registro:

- `actions.json`: 501 ações (nome, categoria, caption EN, short caption, hint, `ShortCutStr`,
  `ImageIndex` + lista de imagens, `Scope`, `MenuAllowed`, `ToolbarAllowed`, `AutoCheck`,
  `GroupIndex`, `DynVisible`).
- `trees.json`: os 14 módulos com 52 menus, 919 itens, 145 separadores, 61 submenus,
  23 popups, 13 barras e 259 botões. Itens específicos do Mac são marcados como `fora_windows`.
- `strings.pt-BR.json`: captions e dicas vindas do `BCompare.tr`.
- `defaults.json`: itens ocultos por padrão (asterisco da ajuda), conferidos em execução.

### 5.2 Gerar a UI a partir da especificação

- Um gerador (`build.rs` ou `tools/gen_menus.py`) produz em Rust o enum `ActionId`, as tabelas
  de árvore e os atalhos.
- Um único `ActionRegistry` em tempo de execução: `ActionId → (executar, habilitado?, marcado?, visível?)`.
- Um renderizador comum de menus, barras e popups substitui os blocos manuais de `home.rs`,
  `text_compare.rs`, `folder_compare.rs`, `hex_compare.rs` e `session_menu.rs`.
- Ação sem implementação fica visível e **desabilitada**, nunca com comportamento inventado.
- Composição por visão (**CONFIRMADO ESTATICAMENTE** em 16/10/2026, ver
  [MENUS-REGRA-COMPOSICAO.md](MENUS-REGRA-COMPOSICAO.md)): é herança visual de DFM, não fusão em
  tempo de execução. Texto = Base + Session + File + Text; Pastas = Base + Session + DirBase + Dir;
  Sincronização = Base + Session + DirBase + DirSync; Hex, Tabela, Imagem, MP3, Registro e Versão
  = Base + Session + File + módulo; Home = Base + Home. A fusão é por (nome, tipo), com âncoras.
  Depois vem a poda em `DataModuleCreate`: `MergeFile`, `Mac`, `MacTools` e `MacWindow` são
  removidos no Windows. A ordem efetiva já está em `menus-composed.json`.

### 5.3 Medir o original em execução (Frida via REA)

Sempre na cópia portátil controlada (`.codex/reference-runtime`) e com perfil isolado, nunca no perfil real.

1. Sonda de viabilidade: verificar se a barra de menus é um `HMENU` Win32
   (`GetMenu`, `GetMenuItemCount`, `GetMenuItemInfoW` com `MIIM_FTYPE|MIIM_STRING`).
   Se os itens forem *owner-draw*, o texto não vem pelo Win32. Nesse caso, percorrer os
   objetos Delphi (`TUiAction`/`TUiMenuItem`) com os offsets de RTTI já decodificados.
2. Para cada contexto (Home, Texto, Pastas, Sincronização, Mesclagem, Hex, Tabela, Imagem,
   MP3, Registro, Versão e popups), capturar a árvore final com ordem, separadores,
   habilitado/marcado/visível e atalho. O resultado vira um JSON *golden*.
3. Capturar a geometria (`EnumChildWindows`, `GetClassName`, `GetWindowRect`) e as fontes
   (`WM_GETFONT`) no mesmo DPI usado pelo AnyCompare.
4. O AnyCompare ganha `--dump-ui <arquivo>` para exportar a própria árvore. Um script compara
   os dois JSON e gera a matriz de diferenças (documento 08).

### 5.4 Oráculo funcional por relatórios do BC4

Para cada comando de uma fatia: preparar fixtures, executar o script no original, guardar o
relatório (`expected`), executar a mesma comparação no AnyCompare e comparar. É o que já
funcionou no texto (5.954 linhas, 0 divergências). A mesma técnica se estende a pastas
(`folder-report layout:xml` com filtros `display-*`), hex, tabelas, imagens, MP3, registro,
versão e `sync`.

### 5.5 Ghidra apenas para lacunas

1. `record_unknown` com a pergunta exata e a autoridade necessária.
2. Uma sessão REA longa por bloco de perguntas. A primeira consulta paga a análise do binário.
3. `search_strings` (texto da UI ou da mensagem) → `xrefs` → `analyze_function` →
   `procedure_callees`/`procedure_references` → `read_bytes` para constantes. Conferir o
   assembly quando o pseudocódigo esconder entradas (lição do DX-Ball).
4. Usar `handlers.json` (RTTI) para saltar direto do evento (`OnExecute`) ao endereço do handler.
5. `export_evidence_bundle` antes de encerrar; `update_unknown` com as evidências obtidas.

### 5.6 Fatias verticais com critério de pronto

Uma visão só é marcada como concluída quando atende a três condições:

- (a) a árvore de menus, barras e popups gerada é igual ao dump do original;
- (b) cada comando visível tem implementação ou está desabilitado de propósito;
- (c) os testes do oráculo da visão passam.

O registro fica em `spec/ledger.json`; `evaluate_reconstruction_coverage` pode servir de
contabilidade formal quando houver evidências REA associadas.

## 6. Ordem sugerida

1. **Infraestrutura**: `spec/`, gerador, renderizador comum, `--dump-ui` e dump do original. Isso resolve a estrutura dos menus de todas as visões de uma vez.
2. **Texto**, a visão mais avançada: completar comandos e popups com o oráculo `text-report`.
3. **Pastas**, **Sincronização** e **Mesclagem de pastas** (`folder-report` XML e `sync`).
4. **Hex**, **Tabela**, **Imagem**, **MP3**, **Registro** e **Versão**.
5. **Mesclagem de texto** (3 vias) e diálogos de opções e sessão (formulários em `forms.json`).

## 7. Próximas ações concretas

1. Reconectar o `rea` e confirmar o Ghidra em `binary_session`.
2. Commit inicial no git.
3. Sonda Frida da barra de menus do original (5.3, item 1) e `record_unknown` da regra de fusão.
4. Criar `spec/` e o gerador. Provar o fluxo primeiro na Home (`THOMEACTIONS`, 5 nós + Base/Session).
5. Corrigir `tools/build_rea_dossier.py:265` e gerar os documentos 01–12.

## 8. Riscos e limites

- Pseudocódigo não é o fonte Pascal; ligação estática não prova execução nem estado dinâmico.
- Executar o original altera o perfil dele. Usar sempre a cópia portátil e o marcador `BCState.xml` recriado.
- Ícones, textos e marca são da Scooter Software. Usá-los localmente para estudo e
  interoperabilidade é diferente de redistribuir. Não distribuir o AnyCompare com esses
  recursos sem autorização, e não tocar no código de licenciamento.

## Fontes

- REA: [windows-ghidra-p0.md](https://github.com/morluto/rea/blob/main/docs/windows-ghidra-p0.md),
  [mcp-prompts.md](https://github.com/morluto/rea/blob/main/docs/mcp-prompts.md),
  [native-investigation.md](https://github.com/morluto/rea/blob/main/docs/native-investigation.md),
  [estudo de caso DX-Ball](https://rea.tools/showcase/dx-ball/), [guias](https://rea.tools/guides/);
  referências locais da skill (`native-and-artifacts.md`, `evidence-workflows.md`) e `binary_session`.
- Delphi: [Dhrake](https://github.com/huettenhain/dhrake), [Pythia](https://github.com/nccgroup/pythia),
  [DelphiHelper](https://github.com/eset/delphihelper).
- Beyond Compare: [scripting reference v4](https://www.scootersoftware.com/v4help/scripting_reference.html)
  e a ajuda local `BCompare.chm` (`commands*.html`, `scripting_reference.html`).
