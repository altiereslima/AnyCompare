# Prévia 0.2.3 — Word Wrap no comparador, navegação e idiomas — 05/11/2026

Executável: `dist/preview/AnyCompare-0.2.3.exe`, 24.657.408 bytes.
SHA-256: `a49fa6dd0f4fce6f2fb5e84828ff285589cdb3e900c626c00f94258454d7eaaa`.
Estável 0.2.0 e prévias 0.2.1/0.2.2 preservadas por hash. Não foram abertas
novas janelas nativas e não houve publicação.

**Confirmado estaticamente:** a ajuda `viewtext.html` e `commandstext.html`
do BC5.2.7.32913 descreve Word Wrap no menu View de Comparar Textos. A ajuda
`commandstextedit.html` e o formulário `TTEXTACTIONS` descrevem navegação entre
edições e marcadores numerados. REA conferiu os recursos do executável isolado
(817 recursos, `ev_306b16e7c5e917aa79bf5d1f3fd9980042de50769af266ee32903bc2544bd312`).
O provedor Ghidra do MCP não resolveu o caminho absoluto do Java nesta sessão;
essa inspeção foi de recursos PE, sem decompilação nova ou execução do original.

**Confirmado em execução pelos testes ocultos:**

- **Comparar Textos → Visualizar → Quebra automática de linha** / **Text Compare → View → Word Wrap**.
  Ambos os lados usam a maior altura de cada par alinhado, mantendo os blocos,
  as cores e as faixas de filtro. O cursor e as seleções usam caracteres e
  linhas físicas. O modo é salvo nas opções da sessão e se ajusta ao redimensionar.
- Busca e Ir para rolam até o cursor real em continuações longas. Seleção por
  clique, copiar, edição Unicode, desfazer, somente leitura e salvamento
  UTF-16 LE com BOM e CRLF verificados. Ativar o modo não torna o arquivo sujo.
- Editor independente: Próxima edição/Edição anterior e marcadores 0–9 no
  menu Pesquisar. Alternar, ir para e limpar marcadores funcionam em linhas
  físicas, inclusive com Word Wrap. Inserir linha antes do marcador e desfazer
  deslocam o marcador para a linha correspondente. Navegar não altera o texto.
- Mídia: rótulos e separadores de milhar usam o idioma ativo sem reler os
  arquivos. O menu de importância e a cópia seguem a apresentação atual.
  Na grade do Registro, nomes reais não passam pelo catálogo de tradução; o
  rótulo do valor padrão continua traduzido.

**Decisões de implementação a confrontar:** navegação entre edições usa as
diferenças desde o último salvamento; um marcador em linha excluída é levado
à próxima linha disponível. O comportamento exato do BC5 nesses casos e
seus atalhos/ícones ainda não foram comprovados. Word Wrap no AnyCompare é
funcional no menu, com geometria do BC5 e configuração da barra ainda abertas.

## Validação

- `cargo test --locked`: **179 passaram, zero falharam, 12 ignorados**.
- Oito verificações isoladas: três testes de interface em inglês, troca de
  idioma com sessão de mídia aberta, três casos de regressão de mídia real
  somente leitura e um benchmark de Word Wrap. Todas passaram.
- Benchmark otimizado de CPU, 367.302 bytes, 1.200 linhas e 12 teclas: média
  **24,57 ms**, primeira amostra **95,45 ms**. Não mede GPU ou arquivos maiores.
- Clippy com `-D warnings`, compilação `--release --locked`, formatação e
  verificação do diff aprovadas. O hash da cópia entregue confere com o build.
- Log da suíte: `.codex/bc5-ui-audit/tests-0.2.3.log`; logs isolados e de build
  no mesmo diretório. [Registro da validação](validation/0.2.3/preview-2026-11-05.json).

## Pendências

A [matriz BC5](BC5-PARIDADE.md) separa as correções verificadas das funções que
faltam. Permanecem abertos mesclagem e sincronização, Imagens/Patch, reprodução
de mídia, operações de Registro, seleção avançada de Tabelas, Find in Files,
sintaxe/fontes, tema escuro, perfis e integração com o Windows. Os
[formatos adicionais](FORMATOS-ADICIONAIS-BC5.md) foram revisados; importação e
conversores ainda faltam. A equivalência pixel a pixel não foi verificada nesta
prévia. Nenhum percentual de paridade é inferido da contagem de testes.
