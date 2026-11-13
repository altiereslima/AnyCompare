# Menus contextuais — execução sem janelas, 04/11/2026

Pedido: testar o botão direito em modo oculto. Fonte 0.2.1 em desenvolvimento;
este registro não altera a entrega 0.2.0 em `dist`.

## Método e resultados

**CONFIRMADO EM EXECUÇÃO:** testes de interface no próprio `egui::Context`,
usando eventos de movimento, pressionamento e soltura dos botões do mouse,
cliques nos itens/submenus e Escape. Não houve criação de janelas nativas,
ativação de aplicativos ou injeção de entrada no desktop nesta rodada.

- Sete testes de propriedades passaram: Mídia/Registro/Versões nos dois lados;
  Copiar usa a linha e o lado clicados; Abrir altera somente o painel escolhido;
  Escape e clique fora fecham o menu; clique direito em grupo não o recolhe.
- Os submenus Abrir Com/Explorer passaram por 18 combinações de tipo, lado e
  ação. `TestDialogs` verificou caminho e indicação de selecionar o arquivo ou
  abrir a pasta, sem iniciar Explorer ou aplicativos associados.
- Importância do campo de mídia altera a classificação e pode ser restaurada.
- Copiar no lado ausente, reprodução ainda ausente, operações de escrita do
  Registro e Editar Texto pendente permanecem desabilitados; cliques não executam
  essas ações.
- Os seis testes de Tabelas estão incluídos na suíte geral, incluindo seleção da
  célula clicada com o botão direito e exclusão reversível, além de edição na
  própria célula, cancelamento e codificação ao salvar fixtures temporárias.
- Suíte geral: **152 aprovados, zero falhas, seis ignorados**. Um dos ignorados
  foi executado separadamente abaixo.
- `cargo clippy --all-targets --locked -- -D warnings`: aprovado, sem avisos.
- Teste manual automatizado de mídia real: **um aprovado**, com dois pares
  M4A/MP4 (quatro arquivos autorizados pelo usuário), clique direito e cópia em
  ambos os lados. Caminho canônico, tamanho e SHA-256 foram conferidos antes e
  depois. Nenhum arquivo de mídia foi alterado ou incorporado ao repositório.

Também foi corrigida a posição de 42 traduções que estavam fora do objeto
`translations` no catálogo inglês. O teste de catálogo confirma os rótulos usados
pelos novos menus, sem alterar o idioma global durante testes concorrentes.

Logs locais, não incluídos no controle de versão:

- `.codex/bc5-ui-audit/context-hidden-2026-11-04.log`
- `.codex/bc5-ui-audit/context-user-media-hidden-2026-11-04.log`
- `.codex/bc5-ui-audit/tests-hidden-2026-11-04.log`
- `.codex/bc5-ui-audit/clippy-hidden-2026-11-04.log`

## Editar Texto no Registro

**CONFIRMADO ESTATICAMENTE:** a ajuda original extraída em
`.codex/bc5-help/commandsreg.html` descreve `Open With > Text Edit` como abertura
do arquivo do painel escolhido no editor interno. Não é o aplicativo associado
do Windows nem uma comparação de dois arquivos.

O editor independente continua **pendente**. O submenu agora identifica a opção,
desabilitada, sem criar um atalho falso para Comparar Textos. Teste confirma que
clicá-la não abre um aplicativo nem modifica a sessão. Falta implementar a sessão
real, edição/salvamento/confirmações/codificações e retorno à comparação.

## Limites

Esta rodada conclui **100% dos testes definidos acima**, não 100% do BC5.
Os testes ocultos comprovam os eventos e os despachos da interface; não comprovam
pixels, ClearType, renderização final na GPU, diálogos do Windows ou execução dos
aplicativos externos. Não se repetiu a auditoria visual nem se publicou um novo
executável em `dist`. WAV/Opus e demais formatos continuam na auditoria de mídia.

Para reproduzir, na raiz real do projeto:

```powershell
& C:/Users/Altieres/.cargo/bin/cargo.exe test --locked --target-dir .codex/build-021 property_context -- --nocapture
& C:/Users/Altieres/.cargo/bin/cargo.exe test --locked --target-dir .codex/build-021 property_context_real_user_media_is_read_only -- --ignored --nocapture
& C:/Users/Altieres/.cargo/bin/cargo.exe test --locked --target-dir .codex/build-021
& C:/Users/Altieres/.cargo/bin/cargo.exe clippy --all-targets --locked --target-dir .codex/build-021 -- -D warnings
```
