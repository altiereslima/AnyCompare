# Prévia 0.2.4 de 05/11/2026

A prévia está em `dist/preview/AnyCompare-0.2.4.exe` e tem 24.809.984 bytes.
SHA-256 `e25b16cfe452d65c622391595828356784d3c7c73e9aa17e0da4a83cb573e671`.

Em Tabelas, arrastar, Shift+clique e Shift+setas selecionam intervalos. Ctrl+clique
alterna células individuais. Ctrl+A usa as linhas e colunas visíveis após os
filtros. O botão direito sobre uma célula selecionada mantém o conjunto.

Copiar para a área de transferência exporta o conjunto em TSV com aspas quando
necessário. Uma seleção disjunta exporta as linhas e colunas envolvidas, com
campos vazios nas interseções que ficaram de fora. A cópia entre painéis usa
as correspondências de linhas e colunas. Copiar linhas inclui linhas sem par.
Copiar células exige correspondência para cada célula copiada. Cada alteração
em lote pode ser revertida com um único Desfazer.

A ajuda do BC5 `commandstable.html` distingue cópia de células e de linhas e
descreve o alcance de Selecionar Tudo. Os gestos e a aparência da seleção no
AnyCompare foram implementados com base no modelo da grade atual. A comparação
visual nativa e a rolagem automática durante o arrasto seguem pendentes.
Quando a tabela é realinhada ou filtrada, a seleção volta à célula ativa.

183 testes passaram e 12 ficaram fora da execução padrão. Os quatro testes
novos simularam eventos de teclado, ponteiro e menu sem janelas visíveis.
Clippy com avisos tratados como erros e a compilação de release passaram.
O executável entregue tem o mesmo hash do binário compilado.

[Matriz de paridade](BC5-PARIDADE.md)
