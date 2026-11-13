# Recursos incorporados — 0.1.18

A entrega incorpora todos os 729 payloads autenticados do BCompare.exe em assets/reference/ui-resources.pack. Cada registro preserva índice, tipo, nome, idioma, tamanho e SHA-256. Há também 1.360 imagens derivadas, 200 definições de formulários e 4.974 entradas de texto. O pacote é incorporado por include_bytes e funciona sem acesso aos diretórios de assets durante a execução.

A biblioteca de ícones passou a consumir os dados do pacote. A textura TAB_CLOSE, com quatro estados e quatro escalas, substituiu o X desenhado manualmente nas abas. A altura do controle de abas é lida do formulário TVIEWFORM. O PNG SORT_UP foi conferido visualmente e apresentou fundo incompatível com o indicador do cabeçalho de Pastas. Sua associação foi retirada e o indicador anterior foi preservado. O recurso permanece disponível no pacote para associação ao controle correto.

O comando --verify-resources permite consultar o conteúdo incorporado. A ferramenta tools/pack_reference_resources.py gera o pacote e verifica cada hash contra o inventário autenticado. Com --verify-executable dist/AnyCompare-0.1.18.exe, verifica os dados que efetivamente chegaram à compilação. A geração também interpreta os TPF0 originais e compara as hierarquias dos 200 formulários com as definições incluídas.

A verificação independente confirmou 729 de 729 hashes no executável. O teste de adulteração rejeitou a mudança de um byte. A suíte passou com 98 testes aprovados e dois testes manuais ignorados. A validação de 48 inicializações/capturas e os relatórios de integridade estão em validation/0.1.18.

## Disponibilidade dos dados e funcionamento da interface

Os 729 recursos estão disponíveis como dados de referência. Os consumidores efetivos estão relacionados em validation/0.1.18/resource-bindings.json. Os recursos internos, manifesto, informações de versão e dados OAuth originais permanecem como referência somente leitura. Eles não são registrados como identidade, manifesto ativo ou scripts do AnyCompare. A faixa promocional continua fora da interface.

Os 200 formulários incorporados são definições. Os eventos Delphi e as 501 declarações de ações não são executados pela engine Rust. Comparar Tabelas, mesclagem de três vias, sincronização, comparadores adicionais e os controles e regras associados ainda exigem implementação. Essa distinção vale para a contagem de recursos e não deve ser substituída por uma alegação de 729 funcionalidades concluídas.

O próximo passo continua sendo o Comparador de Tabelas, usando os 18 pares e relatórios BC4 de docs/review/craft-2026-11-02/tables, incluindo as configurações explícitas de delimitador ainda pendentes.
