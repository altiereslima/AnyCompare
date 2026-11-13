# Prévia 0.2.2 — pesquisa do editor — 04/11/2026

Executável: `dist/preview/AnyCompare-0.2.2.exe`, 24.718.336 bytes.
SHA-256: `60d81b141b52345cf74dac0f5dff69ea589efc99d48d5d0026c5560b234c9b27`.
Estável 0.2.0 e prévia 0.2.1 preservadas. Não houve novas janelas nativas ou
interação com o desktop; a preferência de testes ocultos continua válida.

## Mudanças

**CONFIRMADO ESTATICAMENTE na ajuda original:** `commandstextedit.html` descreve
Find/Replace/Find Next/Find Previous/Go To para a sessão Text Edit. A implementação
abaixo usa o mecanismo Unicode/regex do projeto; isso não representa recuperação
do algoritmo proprietário nem aprovação da geometria nativa.

**CONFIRMADO EM EXECUÇÃO por eventos da interface:**

- Menu Pesquisar/Search no editor independente. Ctrl+F abre Localizar; Ctrl+H
  abre Substituir; F3/Shift+F3 navegam entre ocorrências; Ctrl+G usa o diálogo
  nativo de linha/coluna. Escape fecha a faixa e devolve o foco ao documento.
- Pesquisa acima dos caminhos, seguindo a disposição da referência. Opções de
  regex, caixa, palavra inteira e retorno ao início; histórico compartilhado
  com o aplicativo. Digitar na faixa não insere texto no documento.
- Substituir a ocorrência selecionada e avançar para a próxima; Substituir tudo
  funciona como uma alteração única para desfazer. Texto literal com `$` não é
  interpretado como captura; o modo regex aceita capturas. Erros de expressão
  permanecem na faixa e não modificam o texto. Escrita bloqueada em somente leitura.
- Ocorrências vazias em regex avançam entre limites Unicode sem repetir
  indefinidamente a mesma posição. O modo sem retorno ao início é respeitado.
- Pesquisa/Ir para deslocam a área de rolagem até o cursor real, inclusive com
  Word Wrap. Status exibe linha/coluna lógica, sem transformar linhas visuais
  em quebras gravadas no arquivo.
- **Correção de Undo:** pesquisar ou mover o cursor não consome mais o primeiro
  Ctrl+Z de uma edição. O histórico do documento registra alterações de texto;
  os campos de pesquisa mantêm seu próprio Undo. Validado após 110 pesquisas.
  Novo edit após Undo elimina Redo antigo. Histórico limitado por quantidade e
  memória (até 50 alterações/32 MiB, conservando ao menos o último ponto).
- Excluir pelo menu sem seleção remove um caractere Unicode completo.
- Confirmação de salvamento e seletores Abrir/Salvar do editor de arquivo único
  não dizem “esquerdo”. Comparadores mantêm mensagens próprias de cada lado.
  Traduções completas conferidas em inglês; a tradução anterior por modelo não
  era, por si só, evidência de falha de tradução.

## Verificação

- **171 testes aprovados**, zero falhas, nove ignorados na suíte normal.
- **Dois testes isolados de inglês aprovados**: textos efetivamente emitidos
  pela interface do editor/submenu, Find/Replace/Word Wrap, mensagens de regex
  e composição da confirmação de salvar. São testes ocultos de conteúdo/layout,
  não capturas dos diálogos nativos Windows.
- Faixa de pesquisa verificada em 700, 1153 e 1919 pixels de largura. Testes de
  cliques nos botões, foco, caracteres acentuados/CJK, regex, undo/redo,
  rolagem/cursor e somente leitura.
- `cargo clippy --all-targets --locked -- -D warnings`: sem avisos.
- `cargo build --release --locked --target-dir .codex/build-021`: aprovado.
- `cargo fmt -- --check` e `git diff --check`: sem erros.
- Cópia do executável conferida por SHA-256. Os arquivos pessoais do usuário
  não foram editados. Nenhum commit, troca de branch ou publicação foi feito.

Logs em `.codex/bc5-ui-audit/`: `tests-0.2.2.log`, `search-tests-04.log`,
`search-english-04.log`, `clippy-0.2.2.log`, `build-0.2.2.log`.

## Pendências

Pesquisa básica do editor funcional não equivale a todos os comandos do BC5.
Faltam Next/Previous Edit, bookmarks, Find in Files, sintaxe/fontes, regras
avançadas e auditoria visual/DPI. Ainda faltam equivalência de agrupamento de
digitação/IME e benchmarks amplos com arquivos grandes. A equivalência de
pixels/ClearType e Word Wrap em **Comparar Textos** seguem pendentes.

A base de mídia, Registro e outros comparadores mantém as limitações registradas
na [prévia 0.2.1](STATUS-BC5-0.2.1-PREVIA.md) e na [matriz](BC5-PARIDADE.md).
**100% dos testes executados nesta etapa passaram; não significa BC5 100%.**
