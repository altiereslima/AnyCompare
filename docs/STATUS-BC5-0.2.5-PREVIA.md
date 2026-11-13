# Prévia 0.2.5

A prévia está em `dist/preview/AnyCompare-0.2.5.exe` e tem 24.744.448 bytes.
SHA-256 `52897cb259342dd6aa4350d3d7f97ce996d4ffcec5fdf9b4691088fa61f18c4b`.

Em Tabelas, arrastar a seleção até a borda da grade rola a vista nos dois
eixos e estende o intervalo até as células que entram na área visível. A
rolagem vertical mantém as linhas dos dois painéis alinhadas. A rolagem
horizontal acompanha o painel de origem. Soltar o mouse encerra a rolagem.

O teste simula um arrasto até a última célula de uma grade com 45 linhas e
seis colunas, confere a seleção e a parada após soltar o botão, e repete o
arrasto no sentido inverso. A faixa de ativação de 14 pontos lógicos e a
velocidade proporcional à distância da borda são escolhas do AnyCompare.
A comparação com a interação nativa do BC5 segue pendente.

184 testes passaram em modo oculto e 12 ficaram fora da execução padrão.
Clippy com avisos tratados como erros e a compilação de release passaram.
O hash da prévia confere com o binário compilado.

[Matriz de paridade](BC5-PARIDADE.md)
