# Comparar Pastas — 0.1.15

A soltura de duas pastas inicia a comparação. Em solturas separadas, a segunda pasta preenche o lado vazio da sessão ativa. Com os dois lados preenchidos, soltar uma pasta substitui o lado apontado. Os testes exercitam eventos RawInput, verificam os resultados do trabalhador e preservam uma aba de texto com alterações.

O replay portátil do BC4 cobre 18 pares nos dois sentidos, 82 linhas ao todo e 54 relatórios XML considerando Todos, Iguais e Diferenças. As correções preservam o tipo e a grafia de cada lado, o contexto dos subdiretórios e a ordenação natural. O corpus está em `reference/controlled-folder-replay-2026-10-30`.

A suíte passou em 92 testes, com dois testes manuais ignorados. A integração física com o Explorador de Arquivos ainda precisa de verificação. Também continuam pendentes a paridade visual, os critérios avançados, links e junções e os demais comandos de Pastas.
