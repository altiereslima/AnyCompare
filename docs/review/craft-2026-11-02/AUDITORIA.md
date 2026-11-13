# Auditoria para paridade com o BC4 — 02/11/2026

Versão auditada do AnyCompare 0.1.17, commit 036f819. O programa permanece incompleto. Esta auditoria usa o Craft Anything 0.2.0 instalado pelo usuário, o inventário existente e leitura do código atual. Não altera o original, seus arquivos de entrada ou seu perfil.

## O que o Craft acrescentou

O parser nativo verificou os 729 recursos do BCompare.exe, com cobertura declarada completa desse inventário. Foram recuperadas árvores de controles de uma comparação real de texto nos dois aplicativos. A primeira captura foi da janela de avaliação e foi descartada como referência de comparação. A execução seguinte usou o botão Continue normal na cópia portátil.

A comparação visual usou imagens de 1611 × 869 px da mesma dupla de arquivos, com tolerância de 5 níveis por canal. Divergiram 173.972 de 1.399.959 pixels, ou 12,4269%. O índice de layout foi 0,960576 e o índice perceptual 0,889847. Esses índices valem para essa captura e não medem funções, comandos ou paridade geral. As evidências e hashes estão em reference-text.json, anycompare-text.json e text-pixels.json.

O Craft não encontrou o Ghidra em sua própria configuração. Os inventários de aplicações e seus ledgers automáticos descrevem principalmente aplicações JavaScript. Por isso não foram usados como uma prova automática de cobertura do executável Delphi contra Rust. O ledger desta auditoria é uma classificação autoral ligada ao inventário e ao código.

## Inventário completo e limites

O arquivo action-ledger.json e a página index.html incluem as 501 declarações de ações e os 200 recursos de formulários, sem eliminar heranças ou declarações duplicadas. Há 14 declarações específicas de outras plataformas, 92 em módulos ausentes, 46 com uma referência nominal no código e 349 ainda não verificadas. Uma referência nominal não prova funcionamento. Ausência de correspondência nominal não prova que uma ação está ausente. As classificações antigas de 16/10 estão preservadas em campo histórico separado.

## Lacunas prioritárias

| Área | Situação atual | Verificação necessária |
| --- | --- | --- |
| Comparar Tabelas | Módulo ausente. CSV abre como texto | Parser de registros/células, chaves, duplicatas, cabeçalhos, ordenação, alinhamento, regras e edição |
| Mesclar Texto | Comparador de três vias ausente | Ancestral, conflitos, decisões e saída preservada |
| Combinar Pastas | Módulo ausente | Três raízes, operações e conflitos |
| Sincronizar Pastas | Módulo ausente | Direções, exclusões, regras, prévia e execução consistente |
| Demais comparadores | Imagem, MP3, registro, versões, XML e demais sessões não implementadas | Matriz própria por formato e operação |
| Seleção automática de formato | Entradas principais oferecem Texto, Pastas e Hex | Preferência e ordem de formatos, máscaras, conversores, abertura pelo duplo clique e abertura explícita |
| Pastas | Comparação básica presente | Estados agregados dos ícones, colunas configuráveis e ordenação, filtros avançados, links/junções, archives e perfis remotos |
| Texto | Alinhamento e edição presentes | Formatos/gramáticas completos, regras e diferenças importantes, buscas, relatórios e operações em todos os estados |
| Hex | Alinhamento e edição presentes | Ampliar oráculo além dos 30 casos existentes, grupos, navegação, inserção e arquivos grandes |
| Abas | Posição, ícones e fechamento presentes | Arraste, reordenação, menu de contexto, overflow e comportamentos avançados |
| Sessões | Histórico e formatos próprios presentes | Interoperabilidade BC4, nomes salvos, árvore, workspaces e toda a navegação |
| Visual | Diferenças medidas em uma tela | ClearType, scrollbars, campos, menus, submenus, popups, diálogos, foco, seleção, divisórias e todos os DPIs |
| Salvamento | Documentos locais possuem proteção e codificações | Mesma matriz para tabelas, conversores, perfis remotos e operações parcialmente concluídas |
| Desempenho | Existem testes localizados de edição | Digitação, seleção, scroll, filtros e cópia por tipo de arquivo e tamanho |

A faixa vermelha, selo e link promocional continuam excluídos por decisão do usuário.

## Achado no Global.csv

Os dois arquivos informados têm 497.322 bytes e o mesmo SHA-256 736f1286b7dc06e49cd47718c92898af655976a5e1af965277c29db59a0f5baa. O parser CSV de diagnóstico leu 1.010 registros de 12 colunas, com UTF-8 BOM, dez células multilinha e 24 ocorrências repetidas além da primeira chave. Existem 1.030 linhas físicas. Esses números não devem ser usados para deduzir as opções de cabeçalho e filtragem que o BC4 aplicou à tela do usuário.

O AnyCompare oferece três tipos de sessão e abre o par como Texto. O fluxo de arquivos abertos a partir de Pastas também solicita Texto. Portanto a igualdade binária pode estar correta enquanto a visualização e as operações tabulares estão ausentes. O próximo módulo deve preservar ocorrências repetidas, em vez de sobrescrever registros por chave em um dicionário.

## Matriz de tabelas executada no original

Foram criados 18 pares sintéticos e obtidos 18 relatórios data-report do BC4, com saída zero. Há CSV idêntico, célula alterada, linhas reordenadas, inserção/exclusão, chaves duplicadas, delimitadores entre aspas, aspas escapadas, células multilinha, células vazias e finais, ponto e vírgula, TSV, tabulação dentro de aspas, UTF-8 com e sem BOM, UTF-16, Windows-1252, LF, ausência de cabeçalho, larguras irregulares e representação numérica. Alguns pares combinam mais de uma dessas condições.

A amostra com ponto e vírgula mostrou que a configuração CSV padrão do original separou a vírgula decimal. Ela exige uma sessão nativa com delimitador explicitamente configurado. Produzir um relatório não significa aprovar a interpretação dos campos. XLS/XLSX/ODS, largura fixa, conversores, tolerância numérica e mapeamento diferente de colunas ainda precisam de casos próprios.

Os casos, hashes e resultados estão em tables/manifest.json e tables/results.json. Os relatórios nativos preservam números originais de linha e resultados de alinhamento. A sintaxe foi conferida na referência oficial de scripts do BC4, que define data-report para a sessão de tabelas.

https://www.scootersoftware.com/v4help/scripting_reference.html

## Critério de conclusão

Cada requisito precisa de entrada, opções nativas, ação, resultado de referência, resultado do AnyCompare e uma verificação reproduzível. Telas, nomes de comandos e testes dos três comparadores existentes não encerram as funções dos módulos ausentes. O ledger enumera todo o inventário recuperado, mas a cobertura comportamental continua aberta.
