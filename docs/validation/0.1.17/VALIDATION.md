# Verificação da versão 0.1.17

Foram aprovados 97 testes e dois testes manuais permaneceram ignorados. Os mesmos resultados foram obtidos em uma cópia produzida exclusivamente pelo índice Git, depois de restaurar os seis diretórios vazios dos fixtures. O teste nativo de pastas passou a resolver os caminhos dentro do checkout, preservando o corpus original e permitindo executar a suíte em outra localização.

Clippy passou com avisos tratados como erro. As 48 inicializações do script terminaram com código zero e stderr vazio. As quatro capturas novas cobrem abas em PT-BR, inglês, buffer modificado e comparação hexadecimal. Os testes anteriores de geometria também passaram.

As capturas adicionais do Readme.txt usam as mesmas duas pastas da imagem do usuário. A barra de abas foi comparada em 958 px de área cliente. Foram conferidos posição abaixo do menu, altura de 25 px, ícones, bordas, fechamento, título compacto, divisórias verticais e linha horizontal da barra quebrada em duas linhas.

O executável entregue tem 10031616 bytes e SHA-256 B1D7C1B544BF0AB1B26F1252E01BEA348BAFC1E82DB51536C824B1999D7D6DA3. O arquivo versionado e o alias AnyCompare.exe têm o mesmo hash.

Os novos testes de eventos verificam cliques no corpo e no fechamento, consumo único das ações e fechamento da aba Home. Os testes de sessão verificam título comum, asterisco de alteração e seleção da aba vizinha. A confirmação nativa de salvamento segue o fluxo existente. A rodada não comprova arraste/reordenação ou todos os comandos de contexto das abas.

A renderização monocromática das fontes e a paridade completa de controles continuam pendentes.
