# Pausa 13/11/2026 — desempenho (abrir, rolar, editar) e fonte do editor

Pausado a pedido do usuário no meio do trabalho. **Nada foi commitado** e **nada foi
publicado em `dist`**. A fonte ainda está marcada 0.2.5. Leia tudo antes de continuar.

## Pedidos do usuário (textuais)

1. "faça um teste de desempenho com os arquivos em C:\Projetos\BC4\teste de desempenho e
   faça nosso app ser mais rápido que o original tanto pra abrir como rolar pra cima e pra
   baixo e editar" — depois: "entenda como ele faz isso e faça melhor".
2. "o nosso tem que mostrar o tempo que demorou pra abrir ... e tem uma barra verde em
   baixo que fica movendo e tem que ser igual".
3. "vai fazendo um gráfico ou números comparativos pra eu ir vendo".
4. "sinto que a fonte no app original é melhor e menos com antisserrilhado".
5. Responder sempre em pt-BR, com porcentagens/números.

## Placar atual (mesmos arquivos: ingles.txt / portugues.txt, 57.108 linhas cada)

| Ação | BC4 original | AnyCompare antes | AnyCompare agora |
|---|---|---|---|
| Abrir (ler + comparar) | **17,8 s** (status do original) | 2,6 s | 2,4–2,5 s |
| Rolar, por passo da roda (app real, medidor externo) | **2,7–5,1 ms** | 516 ms | 15,2 ms (antes do patch de tesselação; re-medir) |
| Rolar, quadro interno (sem GPU) | — | 218 ms | 2,6–3,5 ms |
| Digitar, por tecla (app real, medidor externo) | **12,2 ms** | ~820 ms | 27,4 ms |
| Digitar, quadro interno | — | 130 ms | ~24 ms |

- Medidor externo idêntico para os dois (ver "Ferramentas"): `SendMessage` (síncrono) +
  `RedrawWindow(RDW_UPDATENOW)`, 60 passos da roda para baixo/cima, 20 teclas.
- Quadro interno: `cargo test --release --offline profile_large_pair -- --ignored --nocapture`.
- A máquina tem outras sessões rodando: repetir medidas (variação de até 2×).

## O que o original faz (CONFIRMADO ESTATICAMENTE, evidências já em docs/reference/ghidra-session)

- Mesmo pipeline que o nosso: Myers bidirecional global sobre **chaves de linha inteiras
  de 32 bits** (`native-myers32-middle-snake.json`, limite D = min(f(n,m), 0xFFFFFF),
  falha → rotina alternativa pela vtable +0x10), depois varredura `BcStandardAlign`
  (0x11526b0) e pareamento por proximidade nas faixas não casadas.
- Os dois produzem 14.064 seções neste par (CONFIRMADO EM EXECUÇÃO no status do original).
- O original leva 17,8 s para abrir; nós 2,4 s, dos quais **2,08 s são o Myers** sobre
  `&str`. Ideia pendente (resultado tem de ficar idêntico): internar linhas em ids `u32`,
  laço apertado sem checagem de limites, e paralelizar as metades da recursão.

## O que foi alterado (não commitado)

### Editor do comparador de texto (src/ui/aligned_editor.rs)
- Modo sem quebra de linha: galley do documento só com **métricas** (sem malhas),
  construído em paralelo na primeira vez e **atualizado no lugar** na edição
  (só as linhas alteradas; `apply_geometry` só mexe nos `rect`). Texto colorido pintado
  por `paint_rows` apenas nas linhas visíveis (cache de galleys do egui).
- Modo com quebra (word wrap) mantém o caminho antigo (`layout_wrapped`).
- `EditorCache::painted()` usa o diff temporário da edição no mesmo quadro.
- Teste novo `plain_layout_keeps_cursor_metrics_and_edits_rows_in_place` (edição
  incremental == reconstrução; x das métricas == x do texto pintado).

### Diff (src/diff/mod.rs)
- `AlignedLine.left_text/right_text` agora `Arc<str>` (clonar = contador).
- `DiffEngine::update_edit(old_own, new_own, other, own_left, previous)`: caminho rápido
  da edição por comparação de bytes; `update_changes` compartilhado; helpers
  `line_count`, `line_start`, `count_newlines`, `common_prefix/suffix`.
- Teste `edit_update_matches_line_update_for_every_kind_of_edit` (600 edições aleatórias,
  resultado igual a `update`). Passou em release e em debug.

### Tela (src/ui/text_compare.rs)
- `TextView` em cache (`view_cache`, `view_generation`); reaproveita a visão calculada
  pelo editor no commit da edição.
- Sem clones por quadro: `edit_base` substitui o clone de 2 × N bytes; `layout_calls`
  substitui `initial_text`.
- Miniatura: uma cor por pixel físico numa única malha, em cache por revisão; prioridade
  Diferente > órfã > menor > igual (**INFERIDO**: no original a coluna fica toda vermelha).
- `longest()` (maior linha) em uma passada por bytes, dois lados em paralelo.
- Resultado antigo do diff liberado em thread de fundo (> 10 mil linhas).
- `char_to_byte` por blocos (teste `char_to_byte_matches_char_indices_across_blocks`).
- Benchmark novo `profile_large_pair` (ignorado; aceita ANYCOMPARE_BENCH_LEFT/RIGHT).

### egui vendorizado (vendor/egui, patch em Cargo.toml `[patch.crates-io]`)
Cópia do egui 0.29.1 com mudanças marcadas "AnyCompare":
- `TextEdit::disable_undoer()` (o app tem undo próprio; o undoer clonava o texto 2×/quadro).
- texto anterior só clonado com AccessKit ativo; `accessible_value` limita a 64 KiB.
- seleção pintada como retângulos (`paint_text_selection_shapes`), sem clonar o galley.
- galley antigo liberado antes do relayout (permite atualização no lugar).
- `byte_index_from_char_index` por blocos.
- AccessKit: só as linhas visíveis (+ linhas do cursor) viram nós
  (`accesskit_text.rs`). **Achado**: com qualquer cliente de UI Automation ativo na
  máquina, cada quadro recriava 57 mil nós → 516 ms por passo de rolagem.
- `#![allow(warnings)]` no lib.rs (avisos do rustc novo em código upstream).
- Cargo.lock foi regenerado com `--offline` (necessário uma vez por causa do patch).
  Falta documentar em `vendor/egui/ANYCOMPARE-PATCH.md` (como o do epaint).

### epaint vendorizado
- `tessellator.rs`: galley sem vértices e sem sublinhado retorna cedo (a tesselação
  percorria as 57 mil linhas vazias de cada painel). Acrescentar ao ANYCOMPARE-PATCH.md.

### main.rs
- `vsync: false` (com DWM não há tearing; o vsync somava até 16,7 ms por passo).

### Fonte do editor (CONFIRMADO EM EXECUÇÃO 13/11/2026)
- O original com perfil limpo usa **Consolas, LOGFONT −13, ClearType** nos painéis.
  Prova: ampliação da captura e comparação de contornos com GDI (Courier New erra muito);
  o exe tem "Consolas" logo antes de "Courier New" (0x63a6d4 / 0x63a6f4, UTF-16).
- Implementado: família `"Text Mono"` (theme.rs) = face "Table Mono" (Consolas pelo
  caminho GDI hintado do epaint) + fallbacks; `aligned_editor::editor_font()`,
  `FONT_SIZE = 13.0` (Consolas: height_unscaled = 2048 = units/em ⇒ em exatamente 13).
  **Ainda não conferido visualmente** depois da troca.
- Pendências da fonte: (a) ClearType RGB (o atlas do egui é só cinza; precisaria de
  textura por linha renderizada via GDI ou blending subpixel); (b) na captura antiga o
  nosso texto ficava **2 px acima** do original (topo y=148 vs 150) — remedir com Consolas;
  (c) Hex/editor independente continuam em Courier New até serem verificados no original.

## Estado de verificação

- Rodados: os 3 testes novos, `plain_layout`, benchmark grande. **Não rodados** depois
  das últimas mudanças: suíte completa (`cargo test --offline`), testes ocultos/isolados,
  `cargo clippy --all-targets --offline -- -D warnings`, `cargo fmt`.
- Instrumentação temporária ainda no código: procurar `PERFTMP` (src/diff/mod.rs,
  src/ui/aligned_editor.rs, src/ui/text_compare.rs, src/main.rs). Só imprime com
  `ANYCOMPARE_TRACE`/`ANYCOMPARE_TRACE_REPAINT`, mas **remover antes de entregar**
  (há um aviso de variável `t_ui` não usada).
- O benchmark ainda imprime `idle repaint delay/causes` e `shapes/primitives` (diagnóstico;
  pode ficar ou sair).

## Próximos passos sugeridos (ordem)

1. Remover `PERFTMP`; `cargo fmt`; suíte completa; clippy.
2. Conferir a fonte Consolas lado a lado com o original (capturas do medidor:
   `<prefixo>-open.png`) e corrigir o deslocamento vertical.
3. Re-medir o app real (rolagem estava em 15 ms antes do patch de tesselação).
   Se continuar > 5 ms: o tempo restante está no driver OpenGL — o amostrador mostrou
   espera ativa dentro de `wglMakeCurrent`/troca de buffers; **o app parado gasta ~100% de
   um núcleo** (já acontecia na 0.2.0; CPU de 2,8 s a cada 4 s). Avaliar renderizador wgpu
   (DX12) do eframe.
4. Digitação 24 ms → < 12 ms: o custo restante é clonar 57 mil `AlignedLine` por tecla em
   `update_changes` (~3 ms), `TextView::new` temporário (~1,9 ms), três `apply_geometry`
   (~1,3 ms cada), commit (~2,4 ms) e o `TextEdit` (~4 ms com `from_ccursor` O(linhas)).
5. Abrir: Myers com ids `u32` + paralelismo (manter resultado idêntico; testes de replay
   `full_user_samples_match_original_report_colors` e reverso).
6. Pedido 2: o nosso **já mostra** "Tempo de carregamento: 2,46 segundos" no status
   (CONFIRMADO na captura). Falta a **barra verde em movimento** que o original mostra
   embaixo durante o carregamento: capturar o original durante os ~18 s de abertura
   (o medidor pode salvar capturas em intervalos) e reproduzir igual.
7. Atualizar docs/STATUS e AGENTS.md; gerar prévia em `dist/preview` só quando validado.

## Ferramentas criadas

- `tools/perf_harness.cs` — medidor externo. Compilar com
  `C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe -nologo -out:<scratch>\perf_harness.exe -r:System.Drawing.dll tools\perf_harness.cs`
  (no PowerShell; no Git Bash as opções `/x` viram caminhos). Uso:
  `perf_harness <prefixo> <exe> [args]`. Isola APPDATA em `<prefixo>-appdata`.
  `PERF_IDLE_LIMIT_MS=8000` para o nosso app (o driver OpenGL nunca deixa o processo ocioso).
- `tools/stack_sampler.cs` — amostrador de RIP da thread mais ocupada (dbghelp do sistema
  não resolve símbolos Rust; serve para DLLs do Windows/driver).
- Cópia portátil isolada do original: `.codex/reference-perf-runtime` (BC4Reference.exe,
  hash `aba1b98f…38db` verificado, `.tr` PT). **Antes de cada execução copiar
  `BCState.template.xml` para `BCState.xml`**; usar `/solo`; arquivos em `files/` (cópias).
  Ex.: `perf_harness <prefixo> <...>\BC4Reference.exe /solo <...>\files\ingles.txt <...>\files\portugues.txt`.
- Build com símbolos sem tocar no target normal:
  `$env:CARGO_PROFILE_RELEASE_DEBUG="line-tables-only"; cargo build --release --offline --target-dir target\symbols`.

## Regras que continuam valendo

Não modificar o BCompare.exe original nem o perfil dele; executar só a cópia portátil
isolada. Não tocar em licenciamento. Não encerrar processos REA/Codex. Classificar
conclusões (CONFIRMADO ESTATICAMENTE / EM EXECUÇÃO / INFERIDO / DESCONHECIDO / BLOQUEADO).
Não commitar nem trocar de branch sem pedido. Resumos do Desktop são só leitura.
