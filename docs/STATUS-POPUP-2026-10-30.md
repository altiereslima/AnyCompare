# AnyCompare — popup Sessões — 30/10/2026 — 0.1.14

Continuação da 0.1.13; somente BC4 Pro. A versão 4 completa ainda não está pronta.

## O que foi corrigido

- Fundo branco, borda de um pixel e altura conforme as linhas visíveis. Antes
  havia fundo da Home e oito linhas reservadas mesmo com um único item.
- A largura considera também os ramos recolhidos, nomes traduzidos, ícones,
  níveis de indentação e a margem da barra de rolagem. Removido o mínimo de 260 px.
- O limite de Sessões é 30 linhas, não o padrão genérico 8. O grupo de histórico
  não aparece vazio. A lista se adapta à área do aplicativo e mantém rolagem.
- Seleção inicial de Novo desenhada em azul, restrita ao ícone/nome; a Home
  mantém seu desenho anterior. Botão Sessões permanece marcado enquanto aberto.
- Digitação, colagem, IME e atalhos de edição não chegam ao documento atrás do
  popup. As setas, Enter e Escape continuam disponíveis. Typeahead ainda falta.
- Rótulos da árvore pt-BR: Comparar Pastas (F) e Combinar Pasta (L); catálogos
  mantêm Folder Compare / Folder Merge em inglês. Não altera os ícones da Home.

## Evidências originais

**CONFIRMADO EM EXECUÇÃO:** cópia portátil de bytes idênticos ao original,
renomeada BC4Reference.exe para evitar ambiguidade do catálogo Computer Use.
SHA-256 `aba1b98f848a01c3729cfee3e8818ab8e0d947276bc42c9a55c1475c9f8438db`.
Processo 27292, diretório `.codex/reference-sessions-runtime`, BCState próprio;
fixtures do repositório abertas somente para leitura. Perfil original preservado.

Capturas e árvores de acessibilidade em `reference/sessions-runtime-2026-10-30`.
O corpo mediu **196 × 20 px recolhido** e **196 × 218 px expandido**, a 100%.
Clique simples na folha abriu Pastas e fechou o popup; clique no nome Novo
não expandiu; clique na seta expandiu sem fechar. Os arquivos visuais são JPEG
fornecidos pelo Computer Use, portanto não comprovam RGB exato nem ClearType.
O manifesto identifica o processo, hashes dos arquivos e os limites desta prova.

**CONFIRMADO EM EXECUÇÃO via REA/Frida:** varredura somente de leitura encontrou
os dois TUiTreeButton com OnSelect apontando para SessionsComboSelect. Ambos
usam DropDownCount **30**; as árvores mediram 196 × 20 e 196 × 218. Não foram
injetados eventos nem chamados handlers. O código exige o caminho isolado exato.
A evidência Frida tem subject nulo: o SHA do arquivo/processo é registrado
separadamente; não reclassificar a autoridade da evidência.
`tools/observe_reference_sessions.frida.js` também contém leituras de DC de
janelas visíveis; as amostras TUiTreeButton/TDirTreeView não são cores do popup
nem são usadas para justificar cores do tema.

**CONFIRMADO ESTATICAMENTE via REA/Ghidra:** 23 rotinas ligadas ao controle,
com autoanálise focada (sem Function ID, Stack e x86 Constant Reference Analyzer,
além das duas exclusões anteriores). Não analisado código de licenciamento.

- `0xeb6470`: inicializa o dropdown e calcula max(largura do botão, largura da árvore).
- `0xedbed0`: largura considera todos os nós, indentação, ícone, texto e GetSystemMetrics(2).
- `0xeb69a0`: conta nós visíveis; `0xeb6790`: altura = linhas limitadas × altura do item + 2,
  com ajuste de scrollbar horizontal quando presente.
- `0xeb6540` e `0xeb6660`: despacham OnGetCanSelect e OnSelect.
- `0xeb6590`: Enter em nó não aceito alterna expansão e recalcula altura.
- `0xea5e30`: foco, captura do mouse, abertura/fechamento e posicionamento na área de trabalho.

Resultados em `reference/ghidra-session/tree-control-*.json` e
`tree-button-constructor-2026-10-30.json`. Bundle:
`rea/evidence-bundle-r11-tree-control-ghidra-2026-10-30.json`, 23 registros.
Frida: `rea/evidence-bundle-r11-tree-control-frida-2026-10-30.json`.
O snapshot r11 contém evidências, mas zero primitive_entries; não é um projeto
Ghidra reutilizável para novas consultas. close_binary concluiu; o helper próprio
foi interrompido depois de exportar/fechar, sem encerrar o REA do Codex.
A cópia GUI de referência também foi encerrada após a inspeção, sem edições.

## Validação e limites

Seis testes de Sessões conferem clique, grupos, teclas, cancelamento, medidas,
limite de linhas e bloqueio de edição atrás do menu. A comparação geométrica
passa para recolhido/expandido pt-BR; inglês verifica consistência de largura
entre os estados e as alturas, sem alegar replay original em inglês.
A captura de submenus ganhou uma segunda sequência de cliques com frames de
estabilização: a primeira tentativa fotografava antes de expandir. O comparador
rejeitou essa tentativa; o relatório foi preservado como popup-before-capture-fix.json.

**Ainda desconhecido/incompleto:** cores exatas e sombra do tema, ClearType RGB,
realce/foco completo, navegação e busca incremental nativas, seleção/restauração
completa da sessão ativa e política de troca de abas. O popup egui é limitado
ao cliente do app; o HWND original pode ocupar a área de trabalho fora dele.
Menus/campos, arrasto do divisor, edição Hex inline, funções Pro e comparadores
ausentes continuam pendentes. Não declarar Sessões ou o BC4 100% concluídos.

Entrega, hash e checagens finais: `validation/0.1.14/VALIDATION.md`.
