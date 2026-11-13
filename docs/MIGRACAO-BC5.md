# Referência e plano de implementação — BC5.2.7.32913

## Pedido vigente

Entrega atual 0.2.0: `STATUS-BC5-0.2.0.md`. A lista marcada do que funciona e do que falta está em `BC5-PARIDADE.md`. Em 02/11 o usuário ampliou o pedido para inspecionar todas as telas/menus/submenus e atingir 1:1 com BC5; esse objetivo continua aberto.

Em 02/11/2026 o usuário autorizou terminar o básico da base e avançar para o BC5 completo, substituindo a exigência anterior de concluir 100% do BC4 primeiro. Pediu REA e Craft Anything para confrontar os resultados e acrescentou a abertura de DLL em Comparar Versões.

O programa continua sendo AnyCompare. A referência funcional atual é o Beyond Compare 5.2.7.32913 para Windows; não declarar a reconstrução completa enquanto houver áreas ausentes.

## CONFIRMADO ESTATICAMENTE: arquivos locais

Em `C:\Projetos\BC4\BC5` há os instaladores inglês e português. O executável português tem assinatura Authenticode válida e versão 5.2.7.32913. Seu SHA-256 é `682f7f3d05df0196b9fd72d15fd968c816b556ae8683e5628012af91b30c9e81`. É um instalador Inno Setup 6.6.1 de 32 bits, não o executável de comparação.

Os dois instaladores foram extraídos sem instalação pelo InnoUnpacker 2.71.3, obtido do repositório do mantenedor. As ferramentas, os arquivos extraídos e a procedência permanecem em `.codex`. As cópias isoladas de execução são `.codex/bc5-runtime-pt` e `.codex/bc5-runtime-en`. O programa x64 extraído é idêntico nas duas distribuições, SHA-256 `d4753f363c84db3974e4f43d0f8e2972555995943902f4c4b46992e41b30d2a5`; a tradução portuguesa está no módulo `.tr` separado.

REA 6.7.0 e Craft Anything 0.2.0 verificaram independentemente os mesmos **817 recursos**, incluindo hash, posição e tamanho de cada ocorrência. Foram interpretados os **211 formulários**, sem falhas, e inventariadas **515 declarações de ações**. O BC4 tinha 729 recursos, 200 formulários e 501 ações. A diferença bruta de 100 ações adicionadas/86 removidas inclui mudança de nomes e migração entre formulários; não representa 100 funções inéditas.

Evidências: `reference/bc5/runtime-rea.json`, `craft-runtime-resources.json` e `inventory.json`. Reprodução: `python tools/audit_bc5.py`. Os assets ativos anteriores não foram substituídos cegamente pelos recursos BC5. A equivalência visual completa continua aberta.

## Pesquisa oficial e critérios

A [página de novidades](https://www.scootersoftware.com/download/v5whatsnew) relaciona tema escuro, quebra de linhas, múltiplas planilhas/tabelas HTML, Media Compare, integração Windows 11 e DPI por monitor. As alterações 5.1/5.2 incluem descoberta de sessões na Home, atributos adicionais e políticas administrativas. O [histórico 5.2.7](https://www.scootersoftware.com/all/v5changelog) registra manutenção geral, navegação por Tab em Pastas e atualização do 7-zip para 26.04.

Para Tabelas, a documentação descreve [colunas comparadas e múltiplas chaves](https://www.scootersoftware.com/v5help/sessiontablecolumns.html), [tratamento/tolerância por coluna](https://www.scootersoftware.com/v5help/dlgtablecolhandling.html) e [alinhamento de planilhas](https://www.scootersoftware.com/v5help/sessiontablesheets.html). O changelog 5.2 confirma ignorar caixa/espaços no alinhamento de nomes de colunas. Leitura da documentação é evidência documental; não equivale a executar todos os estados do original.

URLs e digests das páginas consultadas estão em `reference/bc5/web-sources.json`. As páginas completas usadas na pesquisa ficam apenas no cache `.codex/bc5-web`.

## Implementado na primeira etapa 0.2.0

| Área | Comportamento implementado | Limite atual |
|---|---|---|
| Versões | Abertura automática de pares PE DLL/EXE/OCX/SYS/CPL/SCR/MUI; árvore Código, Cabeçalho PE, Fixo, Tradução e Variável; filtros, regras, colapsar/expandir, navegação, cópia, sessões | Leitura local de PE32/PE32+; ainda não reproduz toda a composição visual, MUI externo e todos os comandos do BC5 |
| Integridade PE | Código comparado por bytes, cabeçalhos por campos, versões por estruturas; campos adicionais não expostos têm digest separado | Não executar DLL nem usar `LoadLibrary`; não confundir tamanho igual com código igual |
| Colunas de tabelas | Posição, nomes da esquerda ou direita; nomes normalizados sem caixa/espaços; duplicatas preservadas | Alinhamento manual livre de colunas ainda ausente |
| Chaves | Chave principal e adicionais, incluindo duplicatas compostas | Configurações ainda não cobrem todas as regras nativas de alinhamento de linhas |
| Importância | Diferenças importantes/não importantes, azul, colunas ignoradas e botão Menor | Tolerância numérica é opção de sessão; datas básicas por ordem regional já implementadas; tratamento completo por coluna ainda pendente |
| Números | Decimal exato e tolerância configurável, com opção de vírgula decimal | Não inferir que todos os formatos/regiões do BC5 estão concluídos |
| HTML | Múltiplas tabelas locais, entidades, quebras de linha, spans e seletor | Somente leitura, sem execução de scripts; limites/semântica de HTML complexo ainda exigem confronto adicional |
| Planilhas | Abas inferiores, pareamento por ordem/nomes esquerdo/direito, navegação entre pares e resumo de todas as planilhas, inclusive órfãs | Sem gravação de Excel/ODS e sem alinhamento manual livre de planilhas |
| Controles básicos | Redimensionar colunas, ordenar visualização, ocultar colunas iguais, copiar célula ou linha usando a coluna física de destino | Seleção de intervalos/disjunta e editor direto dentro da grade ainda pendentes |

## CONFIRMADO EM EXECUÇÃO: DLL mostrada pelo usuário

REA identificou um recurso RT_VERSION de 932 bytes na `vcruntime140.dll` de 83.784 bytes. O BC5 isolado produziu o relatório Version Compare usado como oráculo. O núcleo Rust confirmou versão fixa `14.0.24210.0`, Microsoft Corporation, CPU I386 e dados de código de 82.356 bytes. Essa contagem exclui os 496 bytes iniciais até o fim do cabeçalho opcional e os 932 bytes do recurso de versão.

Três pares sintéticos próprios, de 32 e 64 bits, também foram executados no BC5: 636/636, 620/620 e 636/620 bytes de dados, respectivamente. Alterar apenas a descrição de versão mantém o Código igual. Alterar um byte fora da versão muda o Código. As fixtures são geradas por `tools/create_bc5_fixtures.py`; nenhuma DLL Microsoft é redistribuída.

Craft capturou as telas nativas e as árvores UIA em `.codex/bc5-captures`. Usar `/solo` para garantir instância própria e `BCState.xml` válido na pasta portátil; XML com raiz errada gera `.err` e não isola as configurações. Não importar o perfil original. Não encerrar processos do usuário.

## Fila necessária para a implementação completa

1. Concluir controles básicos e matriz de operações dos cinco comparadores presentes, com casos de arquivos grandes, falhas e cancelamentos.
2. Reproduzir as regras/configurações reais de formatos e perfis do BC5; ampliar oráculos de alinhamento, colunas, planilhas e codificações.
3. Implementar Text Merge, Folder Merge, Folder Sync, Text Edit/Patch, Picture, Media e Registry, mantendo sessões próprias.
4. Implementar quebra de linhas sem quebrar seleção, offsets, edição e alinhamento; depois tema escuro completo, DPI, foco, abas e demais ajustes visuais.
5. Concluir archives/conversores, perfis remotos, relatórios, integração Explorer/controle de versão e políticas aplicáveis ao Windows.
6. Fechar os cenários de cada comando/campo/diálogo por execução comparada. Recursos presentes e testes locais não fecham a fidelidade geral.

O restante do BC4 continua como base compartilhada, sem alegação de conclusão. Sem faixa vermelha, selo e link promocional, conforme exclusão do usuário. Não analisar/modificar licenciamento. Não reutilizar executáveis, scripts de instalação ou componentes do fabricante como implementação das funções do AnyCompare.
