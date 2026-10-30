# Passagem para a próxima IA — AnyCompare (24/10/2026)

> **Entrega atual 0.1.17.** Continuar por `STATUS-ABAS-0.1.17.md`. A prioridade é completar Comparar Pastas do BC4. Os registros anteriores são históricos.

> **Ordem confirmada pelo usuário em 30/10/2026:** terminar o **BC4 100%**;
> somente depois incorporar novidades do BC5. Isso inclui as funcionalidades
> ausentes, não apenas corrigir os defeitos visuais atuais. Leia `ESCOPO-BC4.md`.

> **Atualização de 30/10/2026 — popup:** entrega atual **0.1.14**.
> Leia `STATUS-POPUP-2026-10-30.md` e `validation/0.1.14/VALIDATION.md`.
> Medidas nativas do popup e limite 30 linhas confirmados; 23 rotinas Ghidra
> novas e leitura Frida do controle. Typeahead, cores/sombra e seleção completa
> continuam pendentes. A automação portátil usa nome exclusivo BC4Reference.exe;
> nome BCompare.exe foi confundido pelo catálogo com a instalação registrada.
> Não usar o perfil original; a referência isolada e o helper próprio já fecharam.

> **Atualização de 30/10/2026 — Sessões:** entrega atual **0.1.13**, leia
> `STATUS-SESSOES-2026-10-30.md` e `validation/0.1.13/VALIDATION.md`.
> Árvore Sessões funcional em Texto/Pastas/Hex, com três testes de interação.
> 80 testes e 33 capturas. Script corrigido para Windows PowerShell 5.1;
> nomes acentuados antes corrompidos não comprovavam menus abertos.
> Quatro handlers de Sessões decompilados; bundle r10 exportado e helper próprio
> encerrado. Faltam evento genérico do controle e fidelidade completa do popup.
> O checkpoint da 0.1.12 é anterior às alterações atuais.

> **Atualização de 27/10/2026:** a entrega atual é a **0.1.11**. Leia
> `STATUS-2026-10-27.md` e `validation/0.1.11/VALIDATION.md`. O usuário retomou
> apontando divisórias ausentes, diferenças de fontes/campos e seleção do Hex
> que fazia a tela saltar. Além desses defeitos, o replay original confirmou
> alinhamento automático binário: comparar somente por posição estava errado.
> Cinco casos binários agora têm pares/contagens validados nos dois sentidos.
> A equivalência completa de fontes, menus e alinhamento binário não está pronta.
> As atualizações anteriores abaixo são históricas.

> **Atualização de 25/10/2026:** leia também `STATUS-2026-10-25.md`. A versão
> atual é a **0.1.10**, com a parte inferior do Texto implementada e testes do
> alinhamento nos dois sentidos (1.263 seções em ambos). A fila restante começa
> no botão Sessões. As limitações visuais/funcionais estão registradas nesse status.
> Os parágrafos seguintes que descrevem a 0.1.9 são o ponto de partida histórico.

Leia este arquivo primeiro. Depois leia o fim de `docs/STATUS-2026-10-16.md` e `docs/RETOMADA-2026-10-17.md`.
Converse com o usuário **sempre em português (pt-BR)** e informe o progresso em **porcentagem**.

## 1. O que é o projeto

AnyCompare (`C:\Projetos\BC4\anycompare`, Rust + eframe/egui 0.29) é a reconstrução do **Beyond Compare
4.4.7.28397** (referência: `C:\Projetos\BC4\bc4\BCompare.exe`, SHA-256
`aba1b98f848a01c3729cfee3e8818ab8e0d947276bc42c9a55c1475c9f8438db`). O alvo é a **edição Pro**. A meta
do usuário é ficar **"irreconhecível"**: indistinguível do original pixel a pixel, tanto no visual
quanto no funcionamento.

Versão atual: **0.1.9** em `dist\AnyCompare.exe`, SHA-256 `55f9296b4cf661fa442711d1ae241aefcaa89f124e343f58421f41f5adb6a361`.
Ela passou na validação de 22 telas e em 66 testes, com clippy limpo. **Nada foi commitado.**

## 2. Regras do usuário (obrigatórias)

- Não modificar o `BCompare.exe` original nem o perfil dele. Executá-lo só numa cópia portátil isolada.
- Não analisar nem alterar código de **licenciamento** (BcLock/BcVersion).
- Não encerrar processos REA do Codex (ChatGPT/codex.exe).
- Classificar cada conclusão como CONFIRMADO ESTATICAMENTE, CONFIRMADO EM EXECUÇÃO, INFERIDO, DESCONHECIDO ou BLOQUEADO.
- Não fazer commits nem trocar de branch sem pedido.
- Não recriar a faixa promocional da Home, o selo da Scooter Software, o link "Nova versão disponível…"
  nem o painel web.
- Não deixar ações "aparentemente funcionais" sem implementação: o que não existe fica desabilitado (cinza).
- Os arquivos `resumo.txt` e `resumo 1.txt` do Desktop são só para leitura. A validação confere o hash deles.

## 3. Compilar, testar, entregar

Use `C:\Users\Altieres\.cargo\bin\cargo.exe` (não altere o PATH global).

```
cargo fmt
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo build --release --locked
```

Para entregar:
1. Suba a versão em `Cargo.toml` e `Cargo.lock`.
2. Copie `target\release\anycompare.exe` para `dist\AnyCompare-X.Y.Z.exe` e `dist\AnyCompare.exe`.
3. Rode `powershell -File tools\validate_release.ps1 -Executable dist\AnyCompare.exe`. Ele gera
   `docs\validation\X.Y.Z\` (22 capturas e `release-smoke.json` com o SHA).

Capturas reproduzíveis:

```
anycompare.exe --text <esq> <dir> --read-only --size 1617 860 --capture saida.png
anycompare.exe --folder <esq> <dir> --size 1617 850 --capture saida.png
anycompare.exe --home-select 0 --size 1153 723 --capture saida.png
```

A opção `--home-select N` seleciona a N-ésima sessão recente na Home. A captura não grava histórico.

## 4. Método de fidelidade (o que funcionou)

1. **Medir a captura do original** (as do usuário ficam na pasta da sessão ou em
   `docs/reference/screenshots`). Use Python + Pillow: limites de tinta, varreduras de linha/coluna,
   cores exatas.
2. **Confirmar no código original com o REA + Ghidra.** Os DFMs estão em `docs/reference/forms/*.json`;
   as rotinas de layout e de pintura, nos handlers de `docs/rea/handlers.json`.
3. Implementar e comparar numericamente com a nossa captura no **mesmo tamanho de cliente**.
   - Exemplos de script: `tools/compare_toolbar.py`.
   - Cada script compara caixas de elementos, colunas de separadores e linhas de tinta.
4. Armadilhas já resolvidas (não reintroduzir):
   - `theme::apply` precisa de `set_theme(Light)` e `all_styles_mut`. Sem isso o texto padrão sai
     com 12,5 px em vez de 12 px.
   - Ícones são pré-multiplicados em **gama** (`icon_image` em `src/ui/icons.rs`). O egui
     pré-multiplica em espaço linear e clareia as bordas.
   - Linhas de 1 px devem ser `rect_filled` alinhados ao pixel. `rect_stroke` borra em duas linhas.
   - Degradês são pintados por linha em sRGB (`chrome::gradient`).
   - Larguras de texto do egui = avanço GDI (Segoe UI 9pt = 12 px; 10pt = 13 px; 11pt = 15 px).
   - Testes de clique devem avançar o relógio (`render_frame_timed`). Senão, dois cliques seguidos
     viram duplo clique.

## 5. REA

- Configuração: `~/.claude.json` → `mcpServers.rea` = `npx -y rea-agents@6.7.0 mcp`, com GHIDRA_INSTALL_DIR,
  JAVA_HOME etc. Backup em `~/.claude/backups/`.
- Para retomar:
  1. `open_binary` (path do BCompare.exe, `provider_id: "ghidra"`);
  2. `import_evidence_bundle` de `docs/rea/evidence-bundle-home-r5-2026-10-18.json` (o mais recente).
- Dicas Delphi:
  - layout da VMT x64: TypeInfo −0xA8, FieldTable −0xA0, ClassName −0x88, Parent −0x78;
  - propriedades publicadas com offset: `python -I tools/delphi_rtti_props.py TUiEditor`;
  - as strings de assert `...\Source\...\X.pas` localizam a unidade de cada função.
- Lacunas abertas:
  - `unk_7668c82c…`: menus vazios, Visible/Enabled em execução;
  - `unk_8535d836…`: Tabela/XML sem classe própria.

## 6. Fila de tarefas (em ordem de prioridade)

Cada item tem o critério de pronto. Detalhes dos itens 1 e 2 estão em `docs/RETOMADA-2026-10-17.md`.

1. **Texto: parte de baixo e rolagem** (pedido do usuário, adiado). Capturas: original `images/8.png`,
   nosso `images/7.png`, na pasta de sessão do Claude, e `docs/reference/screenshots/text-loaded.png`.
   - Barra fina de rolagem vertical (Windows 11) em **cada painel**, com ▲ ○ ▼ no canto inferior direito.
   - Linha sob cada painel: "26: 1" (linha:coluna), "Texto padrão" e rolagem horizontal.
   - Painel de detalhes: setas ⇨/⇦ do original, `·` nos espaços e `¤` no fim de linha, rolagem
     horizontal com ◀ ▶.
   - Barra de status: ícone ≠, "N seções diferentes", "Iguais", "Inserir", "Tempo de carregamento: X segundos".
   - Conferir a contagem: 1263 no original contra 1264 no nosso com os lados trocados. A contagem
     depende da ordem dos lados?
   - Pronto quando: a captura do nosso bate com a do original nesses elementos (DFM `TTEXTVIEWFRAME` + REA).
2. **Botão "Sessões" da barra** (pedido do usuário, adiado).
   - No original, abre uma árvore suspensa sob o botão com "Novo" e "Salvo automaticamente"; a dica
     é "Botão de Sessões" (captura `images/9.png`).
   - Hoje só volta para a Home: `ToolEvent::*("SessionsCombo")` em `text_compare.rs` e `folder_compare.rs`.
   - Reaproveitar a árvore de `src/ui/home.rs`; duplo clique abre a sessão.
3. **Barra do Hex pelo gerador.** Usar `toolbar_spec::HEX_COMPARE` e `toolbar::show` em
   `src/ui/hex_compare.rs`, como já foi feito em Texto e Pastas. Depois remover `chrome::tool_button`
   se ficar sem uso. É o item mais fácil.
4. **ClearType (texto RGB subpixel).** É a maior diferença visual restante, em todas as telas.
   - O patch fica em `vendor/epaint` (`windows_font.rs`, rasterização GDI hoje em escala de cinza;
     ver `ANYCOMPARE-PATCH.md`).
   - Precisa de atlas RGB e mistura por canal no shader ou na pintura.
5. **Home:**
   - "Editar" (ManagePanel, o editor de configurações da sessão) ainda não existe;
   - aparece um quadradinho branco no canto da árvore quando os nomes não cabem;
   - falta a barra horizontal fina do Windows 11;
   - faltam os estados de mouse por cima.
6. **Barras:**
   - estado quente+marcado (degradê azul, ex.: "Estrutura" em `folder-empty.png`);
   - "Menor" em Pastas depende de comparação por regras.
7. **Funções Pro ausentes** (ver `docs/rea/INVENTARIO-FUNCOES.md`): mesclagem de texto em 3 vias,
   mesclagem e sincronização de pastas, e os comparadores de Tabela, Imagem, MP3, Registro e Versão.
8. **Menus gerados**: trocar os menus escritos à mão por menus gerados de
   `docs/rea/menus-composed.json` (`views[*].default_layout`), como já se faz com as barras.

## 7. Progresso (para reportar ao usuário)

| Frente | Situação |
|---|---|
| REA da composição de menus | 100% (visibilidade padrão confirmada estaticamente) |
| Barras de ferramentas geradas | 2 de 3 telas (Texto e Pastas; falta Hex) |
| Home | layout e comportamento do original. Faltam "Editar" e detalhes de mouse por cima |
| Ações (inventário de 501) | ~27 completas e ~100 parciais na última contagem (rodar `tools/build_function_inventory.py` para atualizar) |

## 8. Arquivos principais

- `src/ui/toolbar.rs` + `src/ui/toolbar_spec.rs`: barras. O gerador é `tools/build_toolbar_spec.py`
  (lê `docs/rea/menus-composed.json` e o `BCompare.tr`).
- `src/ui/chrome.rs`: menus, degradês e faixa de menu de 27 px.
- `src/ui/home.rs`: Home completa.
- `src/history.rs`: histórico com data `saved`.
- `src/ui/text_compare.rs`: Texto.
  - Margem: o clique fora da seta seleciona a linha. Só o quadro da seta de 14 px copia
    (REA `UiEditorWindow.pas`).
- `tools/compose_rea_menus.py`, `tools/rea_eval_actions.py`, `tools/delphi_rtti_props.py`: ferramentas REA.
- `docs/rea/MENUS-REGRA-COMPOSICAO.md`: as regras confirmadas, com os IDs de evidência.

## 9. Achados do REA de 24/10/2026 (registrados um a um, para não se perderem)

### 9.1 REA 6.7.0 não importa os bundles antigos
- O servidor `rea` agora roda a **6.7.0**. `import_evidence_bundle` dos bundles exportados pela 6.5.0
  (`docs/rea/evidence-bundle-*.json`, r1–r5) falha com `evidence_integrity_mismatch`.
- Use esses JSON só como **documentação** (pseudocódigo e IDs ficam legíveis). Refaça as consultas
  quando precisar de evidência nova.

### 9.2 Estrutura do `TTextViewFrame` (DFM `docs/reference/forms/TTEXTVIEWFRAME.json`) — CONFIRMADO ESTATICAMENTE
- `InThumbnail` (alLeft, 39 px no projeto; escalar ×15/13): miniatura à esquerda.
- `MainPanel` → `ComparePanel` (alTop):
  - `InputPanel`, com `LeftPanel` / `Splitter` / `CenterPanel` / `CenterSplitter` / `RightPanel`;
  - `DetailsPanel` (alBottom, 72 px no projeto, `BevelOuter=bvLowered`, `Color=clBtnShadow`): as duas
    linhas de detalhe de baixo.
- `OutputSplitter` + `OutputPanel` (com `OutThumbnail`): saída da mesclagem.
- `StatusBar` (`TUiStatusBar`) sem painéis no DFM: os painéis são criados em código.
- Os **editores**, a **linha sob cada painel** ("26: 1", "Texto padrão") e as barras de rolagem também
  são criados em código. As rotinas de layout (endereços em `docs/rea/handlers.json`) são:
  - `MainPanelResize` 0x1d64720
  - `InputPanelResize` 0x1d64800
  - `EditorPanelResize` 0x1d648c0
  - `DetailsPanelResize` 0x1d64900
  - `SplitterMoved` 0x1d64a30
  - `CenterSplitterMoved` 0x1d64b00
  - `OutputSplitterMoved` 0x1d64b70
  - thumbnails: `InThumbnailGetColors` 0x1d6d140, `ThumbnailRequestMoveBy` 0x1d6d800,
    `InThumbnailRequestMoveTo` 0x1d6d9a0
  - margem: `EditorGutterBtnClick` 0x1d6bd20, `EditorGetGutterBtnHit` 0x1d6b580

### 9.3 Textos da barra de status e da linha do painel (strings) — CONFIRMADO ESTATICAMENTE
- "Tempo de carregamento:": `BCompare.tr` @0x1e23e; em inglês, "Load time:" no exe @0x2147e30, perto
  de "Merge center/left/output".
- "Texto padrão" (nome do formato, à direita de "26: 1"): `BCompare.tr` @0x20dc2.
- "Inserir" (modo de digitação Inserir/Sobrescrever): `BCompare.tr` @0x8a9a.
- "Iguais": `BCompare.tr` @0x18590, junto de "Diferenças", "Órfãos na Esquerda" e "Órfãos na Direita".
  É o estado da linha atual mostrado na barra de status.

### 9.4 Botão "Sessões" = `TUiTreeButton` (herda de `TUiComboTree`) — CONFIRMADO ESTATICAMENTE (RTTI)
- `python -I tools/delphi_rtti_props.py TUiTreeButton` → VMT 0xeb4dc0. É a mesma VMT
  `PTR_FUN_00eb4dc0` que o laço de visibilidade da barra 0x19484a0 testa com `InheritsFrom`.
- Propriedades: `Caption` +0x510, `ImageIndex` +0x518, `ShowCaption` +0x528, `Action` (via
  `TUiTreeButtonActionLink`).
- Herdadas de **`TUiComboTree`**:
  - `DropDownCount` +0x4d0, padrão **8** linhas visíveis na lista;
  - eventos `OnDropDownInit` +0x4e0, `OnDropDown` +0x4b8, `OnGetCanSelect` +0x4f0 (`TUiComboTreeGetCanSelectEvent`),
    `OnSelect` +0x500, `OnCloseUp` +0x4a8.
- Ou seja, o botão abre uma **árvore suspensa** (combo de árvore) com até 8 linhas visíveis.
  `OnDropDownInit` monta os nós (as sessões, como na Home), `OnGetCanSelect` decide quais nós
  podem ser escolhidos (provavelmente só sessões, não pastas: INFERIDO) e `OnSelect` abre a
  sessão.
- Próximo passo para quem implementar:
  - achar quem atribui esses eventos: buscar escritas nos offsets +0x4e0 / +0x500 de um
    `TUiTreeButton` em `TSessionViewFrame` (handlers em `docs/rea/handlers.json`, classe
    `TSessionViewFrame`);
  - medir a lista em `images/9.png`.
- **Comportamento descrito no próprio DFM.** A ação `SessionsComboAction` (`TBASEACTIONS`, em
  `docs/rea/forms.json`) tem a dica *"Sessions Button|Focuses on dropdown button listing sessions that
  can be opened in this view."* (em pt-BR, "Botão de Sessões"). Ou seja, a lista mostra **as sessões
  que podem ser abertas na tela atual**: no Texto, sessões de texto, e assim por diante.
  `OnGetCanSelect` deve filtrar pelo tipo (INFERIDO pela dica). Clicar no ícone foca/abre a lista.
- Os handlers da árvore da Home (o mesmo conteúdo) estão em `docs/rea/handlers.json`:
  - `SessionsTreeViewDblClick` 0x18f5680
  - `SessionsTreeViewChange` 0x18f55e0
  - `SessionsNewListClick` 0x18f70e0
  - `SessionsDeleteClick` 0x18f7d70
  - `SessionsSearchEditChange` 0x18f6310
  - `SessionsFolderOpenClick` 0x18f6f10
  - `SessionsClearClick` 0x18f7870
  - `SessionsRenameClick` 0x18f81b0
  - `SessionsEditClick` 0x18f81d0

### 9.5 Layout do painel de Texto (REA 6.7.0, evidência `ev_eb63fa761b507c292ea9bb777c85298ff85a1ccd1ca77dfffe184ed1232d6621`) — CONFIRMADO ESTATICAMENTE
- **`DetailsPanelResize` 0x1d64900**: as linhas de detalhe (as duas de baixo).
  - São editores guardados no frame em +0x8b8 + i×8 (i = 0..3). Só existem para os painéis da
    sessão: slot +0x198(i) da sessão, que vale 1 para os painéis 0 e 1 em Comparar.
  - Largura de cada uma = largura do `DetailsPanel` − 2 (por dentro do bevel `bvLowered`).
  - Empilhamento vertical:
    - 1ª (+0x8b8, esquerda) no topo 1;
    - em 3 vias (0x1d7b580), o centro (+0x8c8) logo abaixo, mais 1;
    - depois a direita (+0x8c0) em `topo_anterior + altura + 1`;
    - na mesclagem (0x19662c0), a saída (+0x8d0) abaixo dela.
  - Há **1 px de espaço entre as linhas**, que mostra a cor do `DetailsPanel` (`clBtnShadow` =
    (160,160,160)). É a linha cinza entre os dois detalhes na captura do original.
- **`InputPanelResize` 0x1d64800**: divide os painéis por **razão em milésimos**.
  - `LeftPanel` = largura útil × (+0x82c) / 1000; em cima/embaixo, usa a altura.
  - Centro = (+0x86c) / 1000 em 3 vias.
  - Reproduzir isso ao redimensionar a janela. Hoje dividimos ao meio; conferir a razão padrão
    (provavelmente 500, INFERIDO).
- **`EditorPanelResize` 0x1d648c0**: só marca o flag 0x40 em +0x831 e chama um relayout adiado
  (0x10b7a90). A geometria real do editor e da linha "26: 1 / Texto padrão" está nesse relayout, ou
  no posicionamento dos controles criados em código. **Próximo passo:** decompilar 0x10b7a90 e quem
  lê o flag 0x40 de +0x831.

### 9.6 Bundle novo compatível com o REA 6.7.0
- `docs/rea/evidence-bundle-r6-rea670-2026-10-24.json`: 8 registros, as evidências 9.5, 9.7, 9.8, 9.9, 9.12, 9.15, 9.16 (xrefs) e 9.17. Importe este
  arquivo no REA 6.7.0. Os bundles r1–r5 servem só de leitura (ver 9.1).
- Os achados 9.2–9.4 vieram dos DFMs, das strings e do RTTI (`tools/delphi_rtti_props.py`),
  reproduzíveis sem o REA.

### 9.7 Onde está a geometria real do editor (relayout) — CONFIRMADO ESTATICAMENTE
- 0x10b7a90 (evidência `ev_361c11be1aabc0aa1fa8854e59c4bc34464bf7905f2b6623e4d4d579a93b959a`) faz
  BeginUpdate (0x10b8880), chama o **método virtual +0x268** do frame e faz EndUpdate (0x10b88e0).
- Slot +0x268, lido da VMT com `tools/delphi_rtti_props.py` (`find_vmt` + `q(vmt+0x268)`):
  - `TTextViewFrame` (VMT 0x1d52d10) → **0x1d74fd0**. Aqui deve estar a posição do editor, da
    linha "26: 1 / Texto padrão" e das barras de rolagem de cada painel.
  - `TFileViewFrame` (VMT 0x18b5d88) → 0x18c1aa0.
  - `TSessionViewFrame` (VMT 0x1902050) → 0x1908a40.
- **Próximo passo para a IA seguinte:** `procedure_pseudo_code 0x1d74fd0` e as funções que ela
  chama. Anotar alturas, margens e quem cria os rótulos de posição e formato. Depois medir
  `images/8.png` e comparar.

### 9.8 O relayout do Texto 0x1d74fd0 (evidência `ev_f90c347f86354293d9c287f4093980306607739acb96bf324c8b28d9ac6939f9`) — CONFIRMADO ESTATICAMENTE
Funciona por bits de "sujo" no byte +0x831 do frame (0x7f = tudo). Setters VCL usados:
- 0x82c900 = Align (2 = alBottom, 4 = alRight);
- 0x82cdf0 = Left;
- 0x82ce50 = Top;
- 0x82ceb0 = Width;
- 0x82cf10 = Height;
- 0x82dd40 = Visible;
- 0xed94b0 = posição de divisor.

Bits:
- **bit 1 (altura do `DetailsPanel`, `param_1[0xfe]`)** = altura do detalhe esquerdo (`[0x117]`) +
  altura do direito (`[0x118]`) + **3** (1 px em cima, 1 entre as linhas e 1 embaixo).
  - Em 3 vias: + centro + 1. Na mesclagem: + saída + 1.
  - Visível conforme 0x1d7b620. Na mesclagem, o `OutputPanel` (`[0xff]`) vai logo abaixo.
- **bit 2 (opções e status):**
  - aplica as opções aos editores `[0x113+i]` e aos detalhes `[0x117+i]` (0x1d747c0);
  - escolhe o par de ações pelo modo em +0x106×8 (0x1d74f20);
  - chama 0x1d79b10;
  - grava na **barra de status, painel 2** (`param_1[0x9a]` = StatusBar; `Panels[2].Text`) o texto
    `PTR_PTR_02137ae8` ou `PTR_PTR_021373f0`, conforme a opção 0xe (modo de digitação). Ou seja, o
    painel 2 é **"Inserir"/"Sobrescrever"**. Os demais painéis (seções, estado da linha e tempo de
    carregamento) são gravados em outro ponto: procurar `FUN_00eda160(panels, 0/1/3)`.
- **bit 4:** sincroniza ações (cópia, filtros etc.). Não é layout.
- **bit 8 (mesclagem):** marca os botões de saída, pelo lado em +0x39.
- **bit 0x40 (painéis dos editores, i = 0..2):**
  - cada painel `[0x110+i]` tem um **divisor de 3 px** `[0x11b+i]` e um **subpainel** `[0x11e+i]`
    embaixo;
  - lado a lado: divisor `alBottom`, altura 3; subpainel `alBottom`, topo = topo do divisor + 1;
  - em cima/embaixo (0x18c5fd0): divisor `alRight`, largura 3; subpainel `alRight`, à direita do
    divisor;
  - posição = tamanho do painel × (+0x87c)/1000.
  - Esse subpainel deve ser o de "Detalhes de Texto/Hex" por painel (ações `ShowTextDetails`/
    `ShowHexDetails`). **Não** é a linha "26: 1 / Texto padrão".
- **Ainda não localizado:** a linha "26: 1 / Texto padrão" sob cada editor e as barras de rolagem
  por painel. Candidatos: 0x1d79b10 (chamado no bit 2) e o próprio editor `TUiEditor` (unidade
  `UiEditorWindow.pas`), que pode ter uma faixa de status embutida. Procurar a string de formato
  "%d: %d" e quem grava "Texto padrão" (`BCompare.tr` @0x20dc2).

### 9.9 0x1d79b10 = destaque da linha atual só no painel ativo (evidência `ev_463665a4e5797ef5aae055800dc5e4110968f3c8c87b45ec4c668ecaa2f76b4d`) — CONFIRMADO ESTATICAMENTE
- Para cada painel existente (0..3) e para cada um dos seus 2 editores (0x1d7b320(frame, painel, k):
  o editor principal e o de detalhe):
  - **painel ativo** (+0x608): liga a opção 0xf do editor (realce da linha atual) e aplica a
    cor obtida de 0x1d7b160 / 0xf16bb0;
  - **outros painéis**: desliga a opção 0xf e usa a cor padrão `PTR_DAT_02133c80`.
- Consequência para o AnyCompare: a moldura/realce da linha atual aparece **só no painel com
  foco**. Conferir na captura `images/8.png`: a linha "Pedra do Sigilo" tem moldura só no lado
  ativo.
- **Não** é a linha "26: 1 / Texto padrão". Ela continua por localizar; próximo candidato: o
  `TUiEditor` (unidade `UiEditorWindow.pas`), procurando quem formata "%d: %d" e quem grava
  "Texto padrão".

### 9.10 O texto "26: 1" não usa string de formato — resultado negativo, útil
- O exe **não** contém "%d: %d", "%d:%d" nem "%u: %u" em UTF-16. "%0:d: %1:d" aparece só em
  "Parsing Error (%0:d: %1:d)".
- Portanto "linha: coluna" deve ser montado por concatenação (IntToStr + ': ' + IntToStr), no
  próprio editor ou no frame.
- Caminhos sugeridos para achar o dono dessa linha e da rolagem por painel:
  1. Pelo RTTI: `python -I tools/delphi_rtti_props.py TUiEditor` e procurar propriedades de status
     ou barra (ex.: `ShowStatus`, `StatusBar`, `ScrollBars`).
  2. No REA: as funções da unidade `UiEditorWindow.pas` (strings de assert) que chamam `IntToStr`
     duas vezes com `': '` no meio.
  3. Em execução (cópia portátil isolada, Frida): observar quem grava o texto "26: 1".

### 9.11 Dono da linha "26: 1 / Texto padrão" e das rolagens = o próprio `TUiEditor` — CONFIRMADO ESTATICAMENTE (RTTI)
`python -I tools/delphi_rtti_props.py TUiEditor` (VMT 0xf102f8) mostra as propriedades publicadas:
- **`StatusBar`** (+0x4c8): cada editor tem uma **barra de status própria**, a linha sob o painel
  com "26: 1" e "Texto padrão".
- **`OnGetEditPosition`** (+0x570): evento que fornece a posição exibida ("linha: coluna").
  Explica por que não há string de formato (9.10).
- **`ScrollBars`** (getter 0xf16ad0, padrão **3** = `ssBoth`): o editor tem as barras vertical e
  horizontal **próprias**, uma por painel, como na captura do original.
- **`LtVertScroll`** (+0x498): barra vertical à esquerda (opção).
- `ShowRuler` e `ShowGapBars` (getter indexado 0xf164b0).

Próximos passos para quem implementar:
1. No REA, procurar escritas em +0x4c8 de um `TUiEditor`: onde o `TTextViewFrame` cria a barra de
   status do editor e com quais painéis (posição, formato e talvez modo).
2. Achar o handler atribuído a +0x570 (`OnGetEditPosition`) e quem grava "Texto padrão" no
   painel seguinte.
3. Medir em `images/8.png` a altura dessa linha, as larguras dos campos ("26: 1" | "Texto padrão" |
   rolagem horizontal) e a barra vertical fina com ▲ ○ ▼.

### 9.12 Quem preenche "26: 1", "Texto padrão" e o estado "Iguais" (evidência `ev_dad1ad183c362e7aafd4d6cc9b399220de3310505ae7cdc956fd6b6443551b6c`) — CONFIRMADO ESTATICAMENTE
**0x1d6c3e0 (atualização do status do Texto).** Para cada painel i existente, com editor
`param_1[0x113+i]`:
- **Campo 0 da barra do editor** (`editor.StatusBar.Panels[0]`):
  - se a linha do cursor (0xf16570) é uma linha de arquivo, o texto é
    `Format(<padrão em 0x1d6c900>, [nº da linha no arquivo (0x118d0f0), coluna (0xf16530) + 1])`,
    ou seja **"26: 1"**;
  - numa lacuna (linha que não existe nesse lado), fica **vazio**.
- **Campo 1 da barra do editor**: o nome do **elemento de formato/gramática** da linha atual
  (0x118d500 → nome via 0xfcf300).
  - Se não houver elemento (−1), usa a string `PTR_PTR_02135b18`, que deve ser **"Texto padrão"**.
  - Em lacuna, fica vazio.
  - Depois redesenha a barra (método virtual +0x160).
- **Barra de status da janela** (`param_1[0x9a]`), **campo 1**: o estado da linha atual (0x18c53e0
  com o tipo da linha, por exemplo **"Iguais"**). Se o tipo for 8, acrescenta um sufixo
  (`PTR_DAT_01d5d7c0`).
- No fim: realce da linha ativa (0x1d79b10, ver 9.9), notificação (0x10b7910) e 0x1978dd0.

Campos da barra de status da janela (juntando com 9.8):

| Campo | Conteúdo |
|---|---|
| 0 | "N seções diferentes" (a confirmar) |
| 1 | estado da linha ("Iguais"…) |
| 2 | "Inserir"/"Sobrescrever" |
| 3 | "Tempo de carregamento: X segundos" (a confirmar) |

**0x1d64d20 (inverter lados)**: troca entre os dois editores os textos de todos os campos da barra
própria, a opção 0x13 e a propriedade indexada 3.

Para implementar no AnyCompare:
- uma barra de status **por painel** com 2 campos, "linha: coluna" e o nome do formato
  ("Texto padrão"), vazios numa lacuna;
- na barra da janela, o estado da linha no campo 1 e "Inserir" no campo 2.

Falta medir larguras e alturas em `images/8.png`, e ver o padrão de formato em 0x1d6c900
(`read_bytes`) para confirmar ": " entre os números.

### 9.13 Separador e sufixo — CONFIRMADO ESTATICAMENTE (bytes do exe)
- O literal em 0x1d6c900 é a string Delphi UTF-16 **": "** (codepage 1200, tamanho 2). O campo é
  então `IntToStr(linha) + ": " + IntToStr(coluna + 1)` → **"26: 1"**.
- O sufixo do estado de linha tipo 8 (`PTR_DAT_01d5d7c0`) é um `resourcestring` (registro
  `PResStringRec`: módulo 0x0213df48, **id 0xECF4 = 60660**). O texto está na tabela de strings do
  exe (inglês) e do `BCompare.tr` (pt-BR). Para ler: bloco RT_STRING = 60660/16 + 1 = 3792, índice
  60660 % 16 = 4.
- **Lido:** o id 60660 é **"Conflict"** no exe e **"Conflito"** no `BCompare.tr`. A linha de tipo 8
  (conflito, na mesclagem) mostra o estado seguido de "Conflito".
- Ferramenta nova: `python -I tools/read_resource_string.py <id> <BCompare.exe> <BCompare.tr>` lê
  qualquer `resourcestring` (tabela RT_STRING) pelo id. Use-a com os registros `PResStringRec`
  (módulo 0x0213df48 + id) que aparecem no pseudocódigo como `PTR_DAT_…`/`PTR_PTR_…`.

### 9.14 Textos exatos resolvidos (resourcestrings) — CONFIRMADO ESTATICAMENTE
| Ponteiro no pseudocódigo | id | Inglês (exe) | pt-BR (`BCompare.tr`) | Onde aparece |
|---|---|---|---|---|
| `PTR_PTR_02135b18` | 64313 | Default text | **Texto padrão** | 2º campo da barra de cada painel, sem elemento de formato |
| `PTR_PTR_02137ae8` | 62030 | Insert | **Inserir** | 3º campo da barra da janela, opção 0xe desligada |
| `PTR_PTR_021373f0` | 62000 | Overwrite | **Substituir** | 3º campo da barra da janela, opção 0xe ligada (modo de sobrescrever) |
| `PTR_DAT_01d5d7c0` | 60660 | Conflict | **Conflito** | sufixo do estado de linha tipo 8 |

Atenção: em pt-BR o modo de sobrescrever aparece como **"Substituir"**, não "Sobrescrever".

### 9.15 Montagem do Texto e barra de status ao carregar (evidência `ev_850ede3595977589ca76f10935ac2d738ffce8d4748ee3e6a3067756cc8b9c6f`) — CONFIRMADO ESTATICAMENTE
**0x1d79be0 (montagem da tela de Texto):**
- **Rolagem de cada editor** (`ScrollBars`, 0xf16b10; 1 = só horizontal, 3 = ambas):
  - padrão: **todos os editores com as duas barras** (máscara 7), como na captura (barra vertical em
    cada painel);
  - com a opção 8 ligada e lado a lado: máscara 2, só o editor da direita tem a vertical (barra
    compartilhada) e os outros só a horizontal;
  - 0xf16a50(editor, i == c), com c = 1 nesse caso, 0 sem, 3 na mesclagem: marca o editor que
    "possui" a barra vertical compartilhada.
- **Opção 0x33** (só Comparar, lado a lado) → 0xf16840 e opção 10 do editor esquerdo. É
  "**Margem do lado direito para o editor esquerdo**", da ajuda (`optionstweak.html`): põe as setas
  de cópia juntas no centro.
- **Proporção dos painéis** (+0x82c e +0x86c, em milésimos; usada por `InputPanelResize`, 9.5):
  - 2 vias: **500** (meio a meio) com os dois arquivos; **250** se só o da direita estiver
    carregado; **750** se só o da esquerda (o lado vazio fica com 1/4);
  - 3 vias: 333/333; 200/400 se faltar o da esquerda; 400/200 se faltar o da direita;
  - opção 7 (fora da mesclagem): proporção salva na configuração 0x16.
- **Larguras dos campos 0 e 1 da barra da janela** (0x6fbe60 = `Width`): campo 0 = +0x870
  (`param_1[0x10e]`) e campo 1 = +0x874. São 0 na sessão de edição de texto.
- Divisores e painéis +0xf9..+0xfd: alinhados à esquerda/direita (lado a lado) ou em cima/embaixo,
  com divisores de 3 px.

**0x18c26f0 (`TFileViewFrame`, fim do carregamento; vale para todas as telas de arquivo):**
- **Último campo da barra** (índice `count − 1`) = resourcestring `PTR_DAT_018b9248` + separador
  `DAT_018c2b74` + tempo formatado (0x895670, ms em +0x210) → **"Tempo de carregamento: 0,69
  segundos"**.
- Campo 0 conforme o tipo de comparação (0x1979060):
  - tipo 4 → texto `PTR_DAT_018b90f8`;
  - tipo 6 → ícone 7 + texto `PTR_DAT_018b9108`;
  - durante o carregamento (estado 1): ícone −1 e textos vazios.
  - Resolver os textos com `tools/read_resource_string.py`, lendo o id no registro.
- A opção de linha de comando `-closeonidle` instala um temporizador que fecha quando ocioso.
- **Textos resolvidos** (`tools/read_resource_string.py`):
  - `PTR_DAT_018b9248` = id 61990 "Load time:" → **"Tempo de carregamento:"**; separador
    `DAT_018c2b74` = **" "** (espaço). O resto ("0,69 segundos") vem de 0x895670 (formatação de
    tempo; conferir a palavra "segundos" e a vírgula decimal).
  - Campo 0, tipo 4 → id 62001 **"Cancelado"**; tipo 6 → id 62002 **"Erro"** (com ícone 7). Fora
    desses tipos, o campo 0 recebe "N seções diferentes" em outro ponto: procurar a escrita no
    campo 0 da barra (`param_1[0x9a]`) no Texto.
- O bundle r6 tem agora 6 registros (9.5, 9.7, 9.8, 9.9, 9.12 e 9.15).

### 9.16 Texto "N seções diferentes" — CONFIRMADO ESTATICAMENTE (tabela de strings)
- id **61577** "%d difference section(s)" → pt-BR **"%d seções diferentes"**. O registro
  `PResStringRec` fica em **0x19ab768**.
- id 61576 "%d conflict section(s)" → "%d seções com conflito" (mesclagem), registro em 0x19ab758.
- Os registros ficam na unidade de sessão (0x19ab…). Quem grava no campo 0 da barra deve ser a
  sessão ou o frame. Próximo passo: `xrefs 0x19ab768` no REA e decompilar a função.

### 9.17 Quem escreve o campo 0 da barra do Texto: 0x1d76630 (evidência `ev_87e9273b704f4b00ef46df3bebdaaeee2e19e8a9659721610e8b91da46cd7d4b`) — CONFIRMADO ESTATICAMENTE
- Obtém o **estado do resultado** (0x19794a0, 0..5) e o **nº de conflitos** (0x1979570).
- **Com conflitos (≥ 1):** ícone **6** + "**%d seções com conflito**" (id 61576).
- **Sem conflitos:**
  - ícone = tabela `PTR_DAT_02130740` → 0x2077f10, por estado:

    | estado | 0 | 1 | 2 | 3 | 4 | 5 |
    |---|---|---|---|---|---|---|
    | índice do ícone | −1 (nenhum) | 1 | 5 | 2 | 3 | 4 |

  - texto:
    - estado **5** → "**%d seções diferentes**" (id 61577) com o nº de seções (0x118d2d0);
    - estado **3**, se a sessão disser que há diferenças só ignoradas (vmt+0x240) →
      "**Diferenças ignoradas**" (id 62027);
    - outros estados → tabela `PTR_DAT_02130ab8` → 0x2077f28. Ela é preenchida **em execução**
      (zerada no arquivo; os textos vêm de resourcestrings carregadas na inicialização).
      **DESCONHECIDO estaticamente**: observar em execução ou achar quem preenche 0x2077f28.
- Grava `Panels[0].ImageIndex` (0xeda030) e `Panels[0].Text` e chama 0x1d6c3e0 (9.12).
- A imagem do ícone vem da lista de imagens da barra. O ícone 4 do estado 5 deve ser o **≠ vermelho**
  da captura. Confirmar qual lista: `StatusBar.Images`, provavelmente `ContentImages` ou `MainImages`.
- Resumo da barra da janela no Texto:

  | Campo | Conteúdo |
  |---|---|
  | 0 | [ícone] "1263 seções diferentes" |
  | 1 | estado da linha ("Iguais"…) |
  | 2 | "Inserir"/"Substituir" |
  | último | "Tempo de carregamento: 0,69 segundos" |

### 9.18 Candidatos aos textos dos outros resultados (tabela 0x2077f28) — INFERIDO
Busca na tabela de strings do exe, com tradução do `BCompare.tr`:

| id | Inglês | pt-BR |
|---|---|---|
| 61717 | Binary differences | **Diferenças no binário** |
| 61718 | Binary same | **Mesmo binário** |
| 61719 | Unimportant differences | **Diferenças não-importantes** |

São ids seguidos, prováveis textos dos estados 1–4 junto com o de "idêntico". **Não confirmado**
qual estado recebe cada um nem qual é o texto do estado 0. Para confirmar:
- achar quem preenche 0x2077f28 (`xrefs 0x2077f28`; deve ser a inicialização da unidade de sessão,
  perto de 0x19ab…); ou
- observar em execução, numa cópia portátil isolada, com arquivos idênticos e com arquivos que
  só diferem em espaços.
Para buscar outros textos por conteúdo, rode o mesmo laço sobre as tabelas RT_STRING (código em
`tools/read_resource_string.py`).
- `xrefs 0x2077f28` (evidência `ev_c685b1a7744db2910d99003291e7a6b0900506d52b3ee4407b96dfc7d21421b3`)
  aponta para 0x18c2083 (`TFileViewFrame`), **0x1980388** (unidade de sessão, provável
  preenchimento da tabela na inicialização), 0x1d76789 (leitura em 9.17) e 0x2130ab8 (o ponteiro).
  **Próximo passo:** decompilar a função que contém 0x1980388. Ela deve mostrar quais
  resourcestrings vão para cada estado 0..5 e confirmar a tabela de 9.18.
