# Comparar Pastas — 0.1.16

Entrega de 30/10/2026 motivada pelas capturas de `bc4` contra `bc4-en`.
O executável versionado é `dist/AnyCompare-0.1.16.exe`. O alias `dist/AnyCompare.exe` contém a mesma compilação. O arquivo 0.1.15 continua sendo a entrega anterior.

A lista inicia com as subpastas recolhidas. A atualização do mesmo par mantém a expansão. O novo teste cobre a abertura, a expansão, a atualização e a troca do par. O corpus nativo de 18 pares e 54 relatórios permanece nos testes.

Foram refeitos os ícones de pasta com as máscaras originais `MainModels`, recoloridas com o roxo observado nos itens exclusivos. Os nomes das pastas ficam pretos e suas datas cinza. Arquivos recebem os marcadores de 5 × 6 px, cores por lado e o indicador de arquivo CHM. Há faixas alternadas de 16 px, seleção pontilhada, separação central e colunas distintas de nome, tamanho e modificação. Os números têm agrupamento conforme o idioma e as datas mostram segundos.

Os caminhos usam a lista suspensa compartilhada, com os ícones originais de carregar, procurar, pasta superior e subir ambos. O lado direito tem navegação para o par anterior e seguinte. As ações navegam e iniciam a comparação. O histórico desses campos pertence à sessão aberta.

O log tem barra de ícones, linhas delimitadas, identificação do usuário e registro de carga. Salvar log abre o seletor nativo. A barra inferior mostra contagem e volume de arquivos visíveis por lado, além do espaço disponível no volume.

A leitura REA/Frida confirmou Segoe UI, altura -12 e peso 400 nos controles de lista, cabeçalho, caminhos e status do original. A evidência está em `validation/0.1.16/rea-font-evidence.json`. O primeiro probe executou antes da criação das janelas e retornou zero controles. A leitura posterior retornou 89 controles examinados e nove combinações distintas de classe e fonte. Esta rodada não acrescentou decompilação Ghidra.

Foram executados 93 testes, com dois testes manuais ignorados, e Clippy com avisos tratados como erro. A validação de inicialização e captura está em `validation/0.1.16/release-smoke.json`. As capturas adicionais `folder-user-pair-960.png` e `folder-user-pair-1918.png` usam o mesmo par de pastas das imagens do usuário.

A equivalência completa continua pendente. Fontes monocromáticas e ClearType diferem. As barras de rolagem, o tamanho configurável e a ordenação das colunas, menus completos, regras avançadas, navegação em arquivos compactados e demais funcionalidades de Pastas ainda exigem trabalho. As pastas com conteúdo dos dois lados precisam de estados agregados completos nos ícones. Esta entrega não conclui o BC4.
