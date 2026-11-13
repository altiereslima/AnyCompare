# Entrega 0.1.18

Foram aprovados 98 testes. Dois testes manuais permanecem ignorados. Clippy passou com avisos tratados como erro.

As 48 inicializações e capturas terminaram com código zero e stderr vazio. As verificações de geometria existentes passaram. Foram inspecionadas as capturas de abas e Pastas. O fundo do PNG SORT_UP divergiu do cabeçalho e sua associação foi corrigida durante a revisão.

O verificador independente leu o executável entregue e confirmou os 729 payloads por SHA-256, índice, tipo, nome e idioma contra o inventário autenticado. Também verificou os hashes de todas as imagens derivadas. O teste que alterou um byte do primeiro recurso foi rejeitado. O comando --verify-resources foi executado a partir de um diretório vazio, confirmou 729 recursos, 1.360 imagens, 200 formulários e 4.974 textos, e terminou sem stderr.

SHA-256 do executável A157125EF8C6FF165F8781B391FB04AD0B92C8BA58AB276FD39BCEC751BB9741. O alias AnyCompare.exe e o arquivo versionado têm o mesmo hash.

A correspondência entre recursos e consumidores ativos está em resource-bindings.json. As definições disponíveis para módulos ausentes continuam exigindo implementação. A rodada valida dados incorporados, carregamento de imagens e os controles alterados. A paridade funcional completa do BC4 permanece pendente.
