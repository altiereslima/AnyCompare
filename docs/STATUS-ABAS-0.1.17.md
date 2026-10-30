# Abas e divisórias — 0.1.17

As abas são desenhadas depois do menu de cada comparador e antes da barra de ferramentas. A faixa compartilhada tem 25 px, ícones originais MainImages, seleção, gradientes, bordas e fechamento embutido. A largura de 286 px foi medida na imagem fornecida pelo usuário. A altura de 25 px e o uso de MainImages estão no formulário TVIEWFORM recuperado.

O REA confirmou o recurso TVIEWFORM de 665 bytes no executável de referência, com SHA-256 4bc0e15602b4ea8eca9ba2d2015861720afdbf3647c2967c9911b9f827315087. A leitura está em validation/0.1.17/rea-tabs-resource.json. O componente é TUiSpeedTabControl, com SquareTabs ativado. Esta observação estática não comprova todos os comportamentos do controle.

A Página Inicial aparece como aba quando sua visualização está aberta. Ela deixa de ocupar uma aba permanente durante a comparação. Arquivos com o mesmo nome nos dois lados usam um título único. Buffers alterados continuam marcados por asterisco. Fechar a aba ativa seleciona a vizinha e mantém a confirmação de alterações não salvas.

Foi acrescentada a divisória horizontal entre as linhas da barra de ferramentas quando os botões quebram de linha. Os separadores verticais entre grupos continuam compartilhados entre os comparadores. O espaço e o gradiente da barra com abas foram ajustados pela captura. A imagem de uma janela de 960 px corresponde a aproximadamente 958 px de área cliente. A opção --size usa a área cliente, o que muda o ponto de quebra de linha perto desse limite.

A captura de Readme.txt foi repetida com os dois diretórios do usuário. Os testes adicionais verificam geometria e posição das abas, seleção, fechamento, estado da Home, título compacto e escolha da aba vizinha. Há 97 testes aprovados e dois testes manuais ignorados.

O código-fonte é preparado para publicação em altiereslima/AnyCompare. Os ícones, traduções, vendor e fixtures necessários estão incluídos. Instaladores, executáveis de referência, builds, caches e saídas históricas de análise ficam fora do Git. As seis pastas vazias dos fixtures são restauradas por tools/restore_fixture_directories.py, documentado no README.

A nitidez ClearType, arraste e reordenação de abas, menu de contexto e demais comportamentos avançados do controle continuam pendentes. O BC4 completo permanece em desenvolvimento.
