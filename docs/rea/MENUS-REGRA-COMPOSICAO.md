# Regra de composição dos menus do BC4 (REA + Ghidra, 16/10/2026)

Alvo: `C:\Projetos\BC4\bc4\BCompare.exe` 4.4.7.28397, SHA-256
`aba1b98f848a01c3729cfee3e8818ab8e0d947276bc42c9a55c1475c9f8438db`.
Provedor: Ghidra 12.1.4 via REA 6.5.0, perfil focado
(`analysis_profile.digest` `2050e81ac06e087a1dec0221161d4859ec245d80e886913ead4c6c1314634e3a`).
Bundle exportado: `docs/rea/evidence-bundle-menus-2026-10-16.json` (80 registros, 4 lacunas,
SHA-256 `360fe9d6ca3db18f8f0835bf202d4b19dd1a57da5f9ea12766a6b831aba2e1d3`).
Pseudocódigo é saída do Ghidra, não o fonte Pascal.

## Resposta curta

Os menus não são fundidos por visão em tempo de execução. Cada visão usa **um** data module `T*Actions`,
e a árvore desse módulo é montada pela **herança visual do Delphi** (DFM herdado). Em seguida,
`DataModuleCreate` aplica uma **poda** (remoção de nós). **CONFIRMADO ESTATICAMENTE.**

```
TBaseActions ─┬─ THomeActions
              └─ TSessionActions ─┬─ TFileActions ─┬─ TTextActions, TDataActions, THexActions,
                                  │                └─ TMP3Actions, TPixActions, TRegActions, TVerActions
                                  └─ TDirBaseActions ─┬─ TDirActions
                                                      └─ TDirSyncActions
```

Fonte da hierarquia: VMT pai no RTTI (`docs/rea/classes.json`). Os 13 DFM derivados têm `ffInherited`
na raiz e no `TUiActionTree` (`docs/rea/forms.json`, `flags = 1`); `TBASEACTIONS` não tem.

## 1. Fusão herdada (`TUiActionTree.Nodes`)

| Elemento | Endereço | Observação | Evidência |
|---|---|---|---|
| `DefineProperties` registra `"Nodes"` | 0xe31d50 | Leitura 0xe321d0, escrita 0xe323e0, `HasData` 0xe31cb0 | `ev_cab5275f…` |
| Leitura | 0xe321d0 → 0xe32000 | Lê lista de chaves e funde sobre a árvore já carregada pelo ancestral | `ev_ea333135…`, `ev_a20cedc2…`, dossiê `ev_7651fb39…` |
| Busca por (nome, tipo) | 0xe31120 | Do fim para o começo; compara nome exato e tipo | `ev_b85c218d…` |
| Inserção | 0xe31ea0 | Insere novo nó na posição indicada | `ev_3813c508…` |
| Chave do nó | 0xe31300 / 0xe317d0 | `"Nome: Tipo"`; nome não pode ter `:` nem espaço; raiz = `"Root"` | `ev_0eed9e3a…`, `ev_3ca2a338…` |
| Tipos | typeinfo 0xe309e8 | `TUiActionNodeKind` = Root, Menu, MenuItem, SeparatorItem, SubmenuItem, PopupMenu, Toolbar, ToolButton, SeparatorButton, SubmenuButton, ComboBox, TreeButton | `ev_7397151a…` |
| Contêineres | palavra 0xe321cc = 0x0273 | Root, Menu, SubmenuItem, PopupMenu, Toolbar, SubmenuButton | `ev_c87fbe8d…` |
| Escrita com ancestral | 0xe32200, igualdade 0xe31570 | Grava só nós novos ou diferentes, e nós iguais seguidos de nó novo (âncora) | `ev_e1ef8082…`, `ev_e1e2100a…` |

Algoritmo de leitura (0xe32000), aplicado na ordem ancestral → descendente:

1. `pos = 0`; para cada nó lido do DFM: `i = IndexOf(nome, tipo)` na lista atual.
2. Se `i < 0`, insere um nó novo em `pos` e faz `i = pos`.
3. Se o tipo é contêiner, aplica o mesmo algoritmo aos filhos.
4. `pos = i + 1`. Nada é removido nesta fase.

Consequência: nós vazios no DFM derivado servem de **âncora**. Por exemplo, `THomeActions` declara
`MergeFile` vazio e depois `View`, então `View` entra logo após `MergeFile`, antes de `Tools`.
A ordem ancestral → descendente é a semântica padrão de `InitInheritedComponent` da VCL
(**INFERIDO**, coerente com as flags `ffInherited`).

## 2. Poda em `TBaseActions.DataModuleCreate`

| Elemento | Endereço | Evidência |
|---|---|---|
| `DataModuleCreate` chama VMT+0x98 e depois a poda | 0x19423a0 | `ev_5d3e1090…` |
| Poda: percorre do último ao primeiro; se VMT+0xa0(nó) for verdadeiro, remove (0xe31e30); senão desce | 0x1942310 | `ev_716eb12b…` |
| Predicado base (VMT+0xa0) | 0x1945280 | `ev_562b83e1…` |
| Nome da ação do nó (`<Nome>Action`) | 0x1945420 | `ev_85ed624c…` |
| Dropdown especial (VMT+0xb0) | 0x1945560 (`DisplayFiltersDropDownBtn`); `TDirActions` 0x1139a20 (+`DirHandlingDropDownBtn`) | `ev_27672309…`, `ev_9333f2b3…` |
| Barra permitida (VMT+0xc0) | 0x1945a40 → 0 em Base/Home; 0x10ba0d0 → 1 de `TSessionActions` em diante | `ev_725eb3f0…`, `ev_3bd643b6…` |
| Override `TDirSyncActions` | 0x113af40: remove também o ToolButton `Swap` | `ev_3bf67285…` |
| Override `TTextActions` | 0x1193950: regras condicionais de popups e botões (ver lacuna 2) | `ev_b96ca868…` |
| Slots VMT+0xa0..+0xc0 dos 14 módulos | leituras de 40 bytes | Base `ev_f8748c23…`, Home `ev_69537a6b…`, Session `ev_c42cf475…`, File `ev_bde6e39b…`, DirBase `ev_fa1a7b44…`, Text `ev_38e80148…`, Data `ev_1f6c6bd9…`, Dir `ev_fed13b78…`, DirSync `ev_4e762d1f…`, Hex `ev_70e7213a…`, MP3 `ev_ffa6c06d…`, Pix `ev_c910ba72…`, Reg `ev_1b870bc6…`, Ver `ev_7f2c9634…` |

O predicado base remove o nó quando:

1. o nó precisa de ação (MenuItem, SubmenuItem, ToolButton, ComboBox, TreeButton, ou SubmenuButton
   cujo `<Nome>DropDownBtn` não é especial) e `FindComponent('<Nome>Action')` falha;
2. é um `Menu` chamado `MergeFile`, `Mac`, `MacTools` ou `MacWindow` (literal `"Mac"` em 0x19453d0,
   `ev_c8ef95cc…`);
3. é `Toolbar` e VMT+0xc0 retorna falso. Por isso a Home não tem barra de ferramentas.

Antes da poda, VMT+0x98 (`TBaseActions` 0x1944900, `ev_979ae42d…`) cria as 13 ações de "Nova
sessão" quando a global `PTR_DAT_02135ae8` é 0, e libera ações-campo conforme edição e plataforma.
Por isso a regra 1 não pode ser simulada só com o DFM (lacuna 1).

## 3. Resultado reproduzido

`tools/compose_rea_menus.py` aplica a fusão e as regras 2 e 3, com os overrides de DirSync, e gera
`docs/rea/menus-composed.json`. A regra 1 aparece apenas como marcação `action_not_in_dfm`.

| Visão (módulo) | Menus no Windows | Barra |
|---|---|---|
| Home (`THomeActions`) | Session, View, Tools, Help | — |
| Texto, Hex, Tabela, Imagem, MP3, Registro, Versão (`T*Actions` ← `TFileActions`) | Session, File, Edit, Search, View, Tools, Help | Main |
| Pastas e Sincronização (`TDirActions`, `TDirSyncActions`) | Session, Actions, Edit, Search, View, Tools, Help | Main |

Conferência com capturas do original (`docs/reference/screenshots/home.png`, `text-loaded.png`):
a Home mostra Sessão, Visualizar, Ferramentas, Ajuda, sem barra, e o Texto mostra Sessão, Arquivo,
Editar, Pesquisar, Visualizar, Ferramentas, Ajuda. Os botões visíveis da barra de Texto aparecem na
mesma ordem da árvore composta (SessionHome, SessionsCombo | FavCompareAll, FavDiff, FavSame |
ShowContext, InsigIgnored | Rules, FileFormat | CopySide, FullEdit | DiffSectionNext/Prev | Swap,
Reload). Os demais botões estão ocultos por padrão. Isso é **coerente com observação anterior**,
não é uma nova captura REA.

Nota sobre a hipótese anterior do guia (seção 5.2): "Texto = Base + Session + File + Text" está
**CONFIRMADO ESTATICAMENTE** como cadeia de herança, não como fusão em tempo de execução. A visão de
pastas é Base + Session + DirBase + Dir (e DirSync). O menu `MergeFile` existe na árvore, mas é
removido no Windows pela regra 2.

## 4. Lacunas registradas (`record_unknown`)

| ID | Pergunta | Severidade | Autoridade necessária |
|---|---|---|---|
| `unk_e1576aa87b199542cdccf603aa0b557e8327475f7b60454899ce90af789fdb2d` | Ações criadas/liberadas em VMT+0x98 (campos +0xb8..+0x1d8, flags `PTR_DAT_02135ae8`, `FUN_00fd5a10`, `FUN_0085bbe0`) | média | shipped-artifact |
| `unk_5ca0afe452308901c54dd342a132a52d1757a9620204c7f322df5f7e762ab923` | Condição de `TTextActions` VMT+0xc8 → 0x19662c0 / +0x178 (comparação × mesclagem? detalhes?) | média | shipped-artifact |
| `unk_7668c82cdfc1f98662142f7476bad80c1375daf2549a4a03fe433ce8503c1a97` | Materialização da árvore em menu/barra/popup Win32 (menus vazios, DynVisible, personalização) | média | controlled-replay |
| `unk_8535d8360ecd6001e2de6eb1726f108c73e402c9b88f95c91e9ef1b555eb0d27` | Qual `T*Actions` cada tipo de sessão instancia (Tabela/XML/Mesclagem) | baixa | shipped-artifact |

## 5. Impacto no AnyCompare

- O gerador de menus deve reproduzir a fusão por (nome, tipo) com âncoras e a poda, e não uma lista
  manual por tela. `docs/rea/menus-composed.json` já é a especificação de ordem por módulo.
- Itens com `action_not_in_dfm` (ex.: `TextCompareNew`…`XMLCompareNew`, `BookmarkGoTo0..9`) são
  ações criadas em tempo de execução. Não devem ser descartados.

## Apêndice — registros do bundle (IDs completos)

| Evidence ID | Operação | Parâmetro |
|---|---|---|
| `ev_7651fb398da22a98ddcd861b09b55ea78d20ca9ac9fa05bde84660db6e84a94f` | analyze_function | `0xe32000` |
| `ev_5ff7904c72719a92cb6de67ba9d457bfc98e9240ff5298c98b122dbbedb8aa70` | procedure_callers | `0xe31120` |
| `ev_01cea48ba3c17dce510788f26c2d252ca6e462d6c3c35341377be907deb14a63` | procedure_callers | `0xe31300` |
| `ev_53fe65e202182fd92c0febb819b5e6ffef9b887f37dd8983b5a4db56e0fd5498` | procedure_callers | `0xe317d0` |
| `ev_a56fb0116b269f2d11834b32a9a28d686226afd5ae520cb311c487b583f72d0b` | procedure_callers | `0xe31c00` |
| `ev_282b11abd58dae1924293f7d8439cddf3133e2bfe0dd9fab9466b34918102c3c` | procedure_callers | `0xe32000` |
| `ev_3bd643b6e8f3bc0114bc8fc2abc3acea7062aa45d62634c25c5e9d0174fb3e99` | procedure_pseudo_code | `0x10ba0d0` |
| `ev_9333f2b3140a31f09dcec40630af40fed432517d3d3fcdc1c9ea17f7ea3ae6f3` | procedure_pseudo_code | `0x1139a20` |
| `ev_3bf6728548eaa1aaea9269f77630bfc60aa2581ee2fa09df90ddb7d117d2ae63` | procedure_pseudo_code | `0x113af40` |
| `ev_b96ca868dc4bd2ca46dedf8a7c8bf8b7d9a7a723c159120b2c67d88b036b574b` | procedure_pseudo_code | `0x1193950` |
| `ev_716eb12ba789cf0db8680d2ce876eafbaf34146c4fa1c839135f0ce44bdf2de3` | procedure_pseudo_code | `0x1942310` |
| `ev_5d3e1090c9c75b2ba9b481c4f29cfba4bbad4d050d613e40bd482c63620248be` | procedure_pseudo_code | `0x19423a0` |
| `ev_979ae42d09eae0e1f3f80a45a75b59da2089eb7b342113b82ab973d951296d80` | procedure_pseudo_code | `0x1944900` |
| `ev_562b83e1880b2fd5323cba94347403aded0ca853153fe9bacd47081ebdef7921` | procedure_pseudo_code | `0x1945280` |
| `ev_85ed624c2d0c34d4f7db4d13dafa0093b542e002aa8cda7c693913809ab7cc21` | procedure_pseudo_code | `0x1945420` |
| `ev_276723098e8bf05f9b8cab7e60659f23662f813f7fa4a6413b00eb840056b873` | procedure_pseudo_code | `0x1945560` |
| `ev_725eb3f03363cf0d514c38d2706cc7bbb2c606d19079f2de02844dcafdd91316` | procedure_pseudo_code | `0x1945a40` |
| `ev_b85c218d0ba9779df9f7276c33d66751727717fb68826adadb729e389e749326` | procedure_pseudo_code | `0xe31120` |
| `ev_d43fe73d1a04ebb4ca81a6760384ce460042aa05b432ba0394f2c1f2639c7cd5` | procedure_pseudo_code | `0xe31180` |
| `ev_0eed9e3abd2578b4e1c5ad9e762843c1323434735efaae643007679cfcbef44f` | procedure_pseudo_code | `0xe31300` |
| `ev_e1e2100a858b8018a212bfbed015ec344dfad047e5f4d8f133255caa045d4bb5` | procedure_pseudo_code | `0xe31570` |
| `ev_e07539aeebc726b893270dd7027b3417395aad1fb3e18fc2f3c641c9d32cdbd7` | procedure_pseudo_code | `0xe31600` |
| `ev_3ca2a3383c7bf0f3d7ca5713aaff9a92c0d6ea353797442401a2454b6ba53576` | procedure_pseudo_code | `0xe317d0` |
| `ev_7362267037bf47cb925c033d73b2539b717aa814ff9850d63063e7e596ec999b` | procedure_pseudo_code | `0xe31c00` |
| `ev_ff751c7e00cf2847e2a346d888fe5f1a9e5a383cd47c51924069f65dc7bd6105` | procedure_pseudo_code | `0xe31cb0` |
| `ev_cab5275fac827d44ad9d0e49366abf9ae7c7ff1b9a23e66f9696c589e723ee72` | procedure_pseudo_code | `0xe31d50` |
| `ev_3813c5084cfa4b1c1cf4d2c0004201e34f1ebdd4cc41d8daf9ddae274a3794d4` | procedure_pseudo_code | `0xe31ea0` |
| `ev_a20cedc2c636a58a33a690cb0c0aed14dfca3486939799c91e070949b5c37714` | procedure_pseudo_code | `0xe32000` |
| `ev_ea333135651e8787c6b6d96f5e8d6f107258351594c1a801ada871ef2c3e45fe` | procedure_pseudo_code | `0xe321d0` |
| `ev_e1ef80828ce62540cb9eb5f5ae1101a53e74fc3e5fd30bae938cc4cb84b676c6` | procedure_pseudo_code | `0xe32200` |
| `ev_c42cf475c3d06c5fd16f1ec72d10efc2289f70cfac808074a28a8a46b1846132` | read_bytes | `0x10b9388 (40 B)` |
| `ev_d47797afd5768015ebb9ecc2b9a4c5f00c8fdc3afc42240e8a4554fed5f856e2` | read_bytes | `0x10b9388 (8 B)` |
| `ev_69537a6b5918a9122e9f742ab7de09c6474188e29e8ba980849d479b9ae006f0` | read_bytes | `0x10e9df8 (40 B)` |
| `ev_691dab2ef2ff4e0251f37138c624b49370c3552ac720219fae67d0c94892df84` | read_bytes | `0x10e9df8 (8 B)` |
| `ev_fa1a7b442ec3211bdf899c3bda7c617064f081a663c4fb86e47db2102b8ac83c` | read_bytes | `0x10f3cd8 (40 B)` |
| `ev_d61d8649f376df76734097c87e013f3385560433b75a6dd9aa3a9d89c63fe54f` | read_bytes | `0x10f3cd8 (8 B)` |
| `ev_fed13b78ac3ebcbc9d962d99eddde1e99d6093084e3459b23471e23270d6fe25` | read_bytes | `0x1135f80 (40 B)` |
| `ev_4e762d1f56da37a807d8bc389ed377de9e0e4c3a4963688890aa2762399948cc` | read_bytes | `0x113aa00 (40 B)` |
| `ev_bde6e39b8d91c93facbd10082b35e4e78a197bde32f20a89fcdf6f8633f2dde7` | read_bytes | `0x113b2f8 (40 B)` |
| `ev_065371955d9c68815203111069ca3e5cd4ee32a509dbe3e7bde8dd5d4ceef142` | read_bytes | `0x113b2f8 (8 B)` |
| `ev_4c2f37442dab6dbee18660b0bd9ff6f2acccda9e5bee718e5e6175caea853e66` | read_bytes | `0x118e528 (8 B)` |
| `ev_38e80148b2b9dce4f4b6f7e620385f5c6c21a8f72ce99f063a9dc0d60b533f7d` | read_bytes | `0x118e530 (40 B)` |
| `ev_47c20ca6dab250572c932e36bc34bff4115142df3c1e69361d57dfebfb2c82db` | read_bytes | `0x118e530 (8 B)` |
| `ev_1f6c6bd9438e078acf9a781d547bec5ca88126d280d52ddb6b27b6498273ae8f` | read_bytes | `0x1778d90 (40 B)` |
| `ev_70e7213a402b200d9c9c06da08e1c316bfc649d56501f39d209f0859df81ca2c` | read_bytes | `0x178f830 (40 B)` |
| `ev_ffa6c06d4dc4dd5e2782206a73ffae177b02799b79414a7c5357d1229eeda30d` | read_bytes | `0x17a2280 (40 B)` |
| `ev_c910ba724f0335586755aa9379c79c86a1ec65d380908425b3cf036be334dff1` | read_bytes | `0x184a8a0 (40 B)` |
| `ev_1b870bc6d69004e29a74ad7712398235192e77011454997573998c7700bdda8a` | read_bytes | `0x185f2d0 (40 B)` |
| `ev_7f2c9634540400c516e20795f4cb2824e278f62b9ee58c25bf244134311d3156` | read_bytes | `0x18665a0 (40 B)` |
| `ev_aa05381afaab307c1b8fe2fea3cc72f69676de290ead3f436700b2fe257624df` | read_bytes | `0x193f5f0 (8 B)` |
| `ev_f8748c230c043e36e7204e77b4049044c3729fcc5f0897384ba5840caac0cf8f` | read_bytes | `0x193f5f8 (40 B)` |
| `ev_325ca1b93c8cb261ddbb2ee604f83db5edb3eef677df3c0e45cc5c3855ce01eb` | read_bytes | `0x193f5f8 (8 B)` |
| `ev_c8ef95cc7323195cb2722dee6701ca1a4a24f8ea0cf31b3b8549eeaa696f86e7` | read_bytes | `0x19453c8 (24 B)` |
| `ev_7397151a5142a86d7a463ba4ee09afa3842fa99e5e7ddab6758b41176240b4dc` | read_bytes | `0xe309e8 (320 B)` |
| `ev_3a869a5bfe2652708c51a3eabf837278b938f54bd9d4baabbd96e94cd1e736b7` | read_bytes | `0xe31538 (48 B)` |
| `ev_6cb3eb44ea7b5e572ce5661665422b8d781a16e84296113aef32c4ae269e1544` | read_bytes | `0xe318f0 (24 B)` |
| `ev_c87fbe8d8e3b26fd82f6f5994a0eb4d44d344c6df04476b9770644a9737a4177` | read_bytes | `0xe321cc (2 B)` |
| `ev_f8a26a6e2e147b6cdfea03738dc88ee1ed186ac38c824f24917beb8b352a6fd1` | record_unknown | `` |
| `ev_bebc33cee4818836c211b5d472fb5e4c470513824e0c384d441ad013629c1844` | record_unknown | `` |
| `ev_c09ff04d932ee3beb5ff9e269a368685772301eee68b8b3ce36a2ceeaa9a973f` | record_unknown | `` |
| `ev_e55f1847b331d87755024c783bcc93afc0a15718f1bc2a405caea9f492140790` | record_unknown | `` |
| `ev_2a6694a9a08e58aa5a41dc9e2c1afa94dfcc1300b97e1d52af2786bcdc88b8a5` | resolve_containing_procedure | `0x19452dc` |
| `ev_277fe8d10c0cfcaae7c37c337fd510ce79863feabb8d04d6e9addac9326edcaf` | resolve_containing_procedure | `0xe311c0` |
| `ev_5cae269237aed52e9e42b8c8a882fc9ba6e244f5afe67e232d850893a3ab4baf` | resolve_containing_procedure | `0xe3134b` |
| `ev_ce6b4ccad15b5454ece223f73cb9a0f9c92c320610db51a19ced4318f48ba04b` | resolve_containing_procedure | `0xe31625` |
| `ev_3ddc9e4d90d7ea19de236c4aa483686ed3b1a3e0418a58b9c99111a9a0c0f8b4` | resolve_containing_procedure | `0xe31844` |
| `ev_3aac0ccf4dfa900e9a6713d67fbd421cc9c15b94e3f16db23dfa55959d877b24` | search_procedures | `^FUN_00e3[0-3][0-9a-f]{3}$` |
| `ev_149f79827aa9085bf1284a38e361151547a3c80228cc04864ff269ec9ebf8b9c` | search_strings | `ActionTree` |
| `ev_bfaed220b12e251a6fd0ddc2a181fc1326e693f8bbb610fe6516796105008275` | search_strings | `\\Source\\(Ui|View|App|Main)[^\\]*\\[A-Za-z]*(Menu|Action|View|Merge)[A-Za-z]*\.pas` |
| `ev_55f9660e87da782702fcb6ede9a88fc760f073b409c316174f94ac1eaef0e068` | search_strings | `\\Source\\Ui\\UiAction` |
| `ev_69b9cab51f9bf6624f25f72c64a23add0a76e460d8a6184fc796088b3ff029ce` | search_strings | `MacWindow` |
| `ev_15fe58ce68ac372dd4db1ef51d1098ad244d065764a06585bed70e218d32251f` | search_strings | `MergeFile` |
| `ev_79531a9bb73b7937d030c8ea599a9809edd41ca6d71b179969c0301ee6dd72e0` | xrefs | `0x118e490` |
| `ev_2c4c895154ab2733073e20cde18c52e3b18a0ff66b2ede626b61e16e02eea357` | xrefs | `0x19453b0` |
| `ev_249555075da3c57d1c0f66186af64e0ee76ad2aa0de800024552831298913ca8` | xrefs | `0x1945824` |
| `ev_82ecadcab8454380f42e810438d0598f767f56a01b7030c7edd92248c8c55639` | xrefs | `0xe31248` |
| `ev_3dc0d7df30b6f672385356e7a7c50ecc469ee2fe10568e23b89e4f10b8e54b36` | xrefs | `0xe31488` |
| `ev_68bc10598f14cff5316ea9c43ce2c3043e480a8373f1453c0754fc4dbe6f6054` | xrefs | `0xe31700` |
| `ev_4ba852d4250c98109e18c175045acf3483d0562125406389462c0987fca47b62` | xrefs | `0xe3193c` |
| `ev_ca047b6260d7e8d99fdff736840f55e19053883d12297c0f6392911cd1ef0029` | xrefs | `0xe321d0` |

## Rodada 2 (16/10/2026, noite): ações dinâmicas, classes reais e modo de Texto

Bundle: `docs/rea/evidence-bundle-menus-r2-2026-10-16.json` (121 registros). Tabelas de campos
publicados lidas do RTTI (`vmtFieldTable` em `vmt-0xA0`; nome da classe em `-0x88`; pai em `-0x78`).

**Classes realmente instanciadas (CONFIRMADO ESTATICAMENTE).** Cada visão usa uma subclasse **sem DFM
próprio**, que só sobrescreve o slot VMT+0xc8, o getter da classe de sessão:

| Subclasse de ações | Pai | Classe de sessão (VMT+0xc8) |
|---|---|---|
| `TDirCompareActions` / `TDirMergeActions` | `TDirActions` | `TDirCompareSession` / `TDirMergeSession` |
| `TTextCompareActions` / `TTextMergeActions` / `TTextEditActions` / `TTextPatchActions` | `TTextActions` | `TTextCompareSession` / `TTextMergeSession` / `TTextEditSession` / `TTextPatchSession` |
| `TDataCompareActions` | `TDataActions` | `TDataCompareSession` |
| `THexCompareActions`, `TMP3CompareActions`, `TPixCompareActions`, `TRegCompareActions`, `TVerCompareActions` | módulo respectivo | `T{Hex,MP3,Pix,Reg,Ver}CompareSession` |

Sem subclasse localizada: Sincronização (`TDirSyncActions` direto?), Tabela (`TableCompare`) e XML
(lacuna `unk_8535d836…`, ainda aberta).

**Modo de Texto (lacuna `unk_5ca0afe4…` resolvida).** `F = 0x19662c0(sessão) = vmt[0x190](3) ||
vmt[0x198](3)` só é verdadeiro em `TTextMergeSession`. O slot +0x178 só vale 1 em `TTextEditSession`
(e +0x168 só em `TTextPatchSession`). O predicado 0x1193950 remove:

- em Compare e Patch: o SubmenuButton `DisplayFilters` e os popups `LeftEdit`, `CenterEdit`,
  `RightEdit`, `OutputEdit` e `InputDetailsEdit`;
- em Merge: o popup `Edit`;
- em Edit: as remoções de Compare e também os popups `DetailsEdit` e `Nav`.

**Ações criadas e liberadas em VMT+0x98 (`unk_e1576aa8…`, parcial).**
- Base 0x1944900 (campos): sempre libera `MacCommandLineTools`, `RaiseException`, `MacWindow*` e
  `MacTabToWindow`. `SourceControlManager` sai se não for Pro (`BcIsPro` 0xfd5a10). `CheckForUpdates`
  sai se o campo +0x58 do objeto ligado a `TCheckForUpdatesDialog` for diferente de 0 ou se o
  `wininet.dll` não carregar. Com a global 0x215d69a = 0 cria as 13 ações `<Tipo>NewAction`
  (0x1944680, categoria `Session`); com 1, libera `SessionNewList`, `TabNew`, `WindowNew`,
  `SessionOpen`, `Workspace*`, `CloseTab`, `SaveSnapshot`, `EditTextFile` e `ViewPatch`.
- A global 0x215d69a vale "beta expirada" || `DAT_0215d69c`. Esta build é release (VERSIONINFO
  4.4.7.28397, `dwFileFlags = 0`), então "beta expirada" é falso. `DAT_0215d69c` é gravado pela área
  `BcLock`/`BcVersion`, que é **licenciamento**: fora de escopo e não analisado. Para o clone vale o
  modo normal (0), que é o observado nas capturas.
- **Alvo do AnyCompare: edição Pro** (decisão do usuário, 16/10/2026). Modelar `BcIsPro` = verdadeiro.
- As rotinas por módulo (Session 0x10b9830, File 0x113c930, DirBase 0x10f4b90, Dir 0x1138410,
  Text 0x1191b00, Hex 0x17900b0, Pix 0x184b150, Reg 0x185fd40) foram decompiladas. Elas dependem dos
  virtuais da sessão (+0x168/+0x170/+0x178/+0x180/+0x190/+0x2e0/+0x2e8/+0x2f8/+0x300/+0x308/+0x318/+0x320),
  e os valores por classe estão em `session_slots` (ver abaixo). Ainda não foram avaliadas campo a
  campo: falta a tabela "campo → condição" e 4 slots não triviais (0x184a5d0, 0x19715e0, 0x197afd0,
  0x197b000).

Valores observados dos slots de sessão (constantes; "fn" = não trivial):

| Sessão | +0x168 | +0x170 | +0x178 | +0x190 | +0x198 | +0x2e0 | +0x300 | +0x318 | +0x320 |
|---|---|---|---|---|---|---|---|---|---|
| TextCompare | 0 | 0 | 0 | 0 | painéis 0–1 | 3 | 1 | painéis 0–1 | fn |
| TextEdit | 0 | 0 | 1 | 0 | painéis 0–1 | 3 | 1 | painel 0 | fn |
| TextMerge | 0 | 0 | 0 | 0 | 1 | 3 | 1 | fn | fn |
| TextPatch | 1 | 0 | 0 | 0 | painéis 0–1 | 3 | 1 | 0 | fn |
| DataCompare | 0 | 0 | 0 | 0 | painéis 0–1 | 3 | 0 | painéis 0–1 | fn |
| HexCompare | 0 | 0 | 0 | 0 | painéis 0–1 | 1 | 0 | painéis 0–1 | fn |
| MP3 / Ver | 0 | 0 | 0 | 0 | painéis 0–1 | 0 | 0 | fn | fn |
| PixCompare | 0 | 0 | 0 | 0 | painéis 0–1 | fn | 0 | fn | fn |
| RegCompare | 0 | 0 | 0 | 0 | painéis 0–1 | 0 | 0 | fn | 1 |

Em `TTextCompareSession`, +0x308 devolve `TTextMergeSession` se Pro e `nil` caso contrário
(0x118d7f0): é o caminho Compare → Merge, que existe só na Pro.

## Rodada 3 (18/10/2026): ações efetivas, árvores por tela e visibilidade padrão

Bundle: `docs/rea/evidence-bundle-menus-r3-2026-10-18.json` (144 registros, 10 cabeças de lacuna). Para
retomar, faça `open_binary` com `ghidra` e depois `import_evidence_bundle` deste arquivo.

**Ações criadas e liberadas por tela (`unk_e1576aa8…` resolvida, CONFIRMADO ESTATICAMENTE).**
`tools/rea_eval_actions.py <bundle> docs/rea/actions-effective.json` lê o pseudocódigo das 9 rotinas
VMT+0x98 no bundle e o interpreta para cada subclasse concreta, com os slots da classe de sessão lidos
do exe. Os 4 slots que faltavam foram decompilados: 0x197afd0, 0x197b000, 0x184a5d0, 0x19715e0, além
de 0x40f9a0 = `InheritsFrom`. As premissas são: Pro, global 0x215d69a = 0 e verificação de
atualização permitida. Regenerado com o bundle r3, o resultado é idêntico ao da rodada anterior.

| Tela | Criadas | Liberadas | Efetivas |
|---|---|---|---|
| Home | 13 | 12 | 45 |
| Comparar Pastas / Mesclar Pastas | 28 / 28 | 44 / 53 | 188 / 179 |
| Sincronizar Pastas | 28 | 22 | 106 |
| Texto: Comparar / Mesclar / Editar / Patch | 43 / 34 / 34 / 33 | 60 / 53 / 126 / 142 | 199 / 197 / 124 / 107 |
| Tabela (Data) / Hex / MP3 / Imagem / Registro / Versão | 22 / 22 / 22 / 22 / 23 / 22 | 20 / 27 / 28 / 44 / 28 / 28 | 132 / 124 / 112 / 109 / 132 / 110 |

**Árvore final por tela.** Em `menus-composed.json → views`, cada subclasse recebe a poda completa: regra
1 com o conjunto efetivo, regras 2–3, Swap da Sincronização e regras de popup de Texto.

**Visibilidade padrão (CONFIRMADO ESTATICAMENTE; BcCommands.pas).**
- RTTI do `TUiAction` (`tools/delphi_rtti_props.py TUiAction`): `Category` +0xb8, `Visible` +0x100,
  `DynVisible` +0x141, `MenuAllowed` +0x158 (padrão True) e `ToolbarAllowed` +0x170 (padrão True).
- 0x1947c50 percorre a árvore, e 0x1947a10 grava na tabela de comandos:
  - `MenuItem`/`SubmenuItem` fora de um SubmenuButton recebem o `MenuAllowed`;
  - os demais nós com ação recebem o `ToolbarAllowed` no byte +0x12 (botões e itens de SubmenuButton).
- Os valores são herdados ao longo da cadeia de DFM: por exemplo, `FavCompareNone` tem
  `ToolbarAllowed=False` em `TFILEACTIONS`. O asterisco "oculto por padrão" da ajuda corresponde a
  `MenuAllowed=False` (357 de 366 casos batem).
- 0x19484a0 decide o botão da barra: aparece se o byte +0x12 estiver ligado **e** a `Category` combinar
  com o modo de filtros (+0x5c):

  | Category | Modo DropDown (0) | Favorites (1) | Toggles (2) |
  |---|---|---|---|
  | `DropDown` | — | sim | — |
  | `Favorites` | sim | sim | — |
  | `Toggles` | — | — | sim |
  | outras | sim | sim | sim |

  O grupo `DisplayFiltersDropDownBtn` (SubmenuButton `DisplayFilters`) só aparece no modo 0. Os itens
  dentro dele não passam pelo teste de Category.
- Modos permitidos, +0x5d (0x1947f20 e 0x1947c50):
  - bit 0 quando existe o SubmenuButton `DisplayFilters`;
  - bit 1 com alguma ação `DropDown`/`Favorites`;
  - bit 2 com alguma ação `Toggles`.
- 0x1948f90 troca um modo indisponível pelo seguinte (0→1→2→0). 0x1906760
  (`TSessionViewFrame.DisplayFilterModeClick`) grava o modo escolhido no clique direito da barra.
- Modo inicial DropDown: **INFERIDO** pela ajuda (`display_filters.html`). No Texto, que não tem
  `DisplayFilters`, ele vira Favorites.
- Separadores aparecem só entre itens visíveis. O código disso não foi localizado. As barras geradas
  batem item a item com três capturas do original (**CONFIRMADO EM EXECUÇÃO** nessas telas):

| Tela | Barra padrão gerada | Captura |
|---|---|---|
| Comparar Pastas | Página Inicial, Sessões ▼ \| Todos, Diferenças ▼, Iguais \| Estrutura ▼ \| Menor \| Regras \| Copiar \| Expandir, Recolher, Selecionar, Arquivos, Atualizar \| Inverter, Parar \| Filtros [*.*], Filtros, Esconder | `folder-empty.png` |
| Sincronizar Pastas | Página Inicial, Sessões ▼ \| Menor \| Expandir, Recolher, Selecionar, Atualizar \| Parar \| Esconder | `folder-sync.png` |
| Comparar Textos | Página Inicial, Sessões ▼ \| Todos, Diferenças, Iguais \| Contexto, Menor \| Regras, Formato ▼ \| Copiar, Editar \| Próxima Seção, Seção Anterior \| Inverter, Recarregar | `text-loaded.png` |

Cada tela tem também, em `menus-composed.json`:
- `display_filter_modes` e `default_display_filter_mode`;
- `default_layout`, que dá a barra de menus e as barras de ferramentas visíveis (com `|` nos
  separadores);
- `default_visible` em cada nó.

**Lacunas após a rodada 3.**

| Lacuna | Estado |
|---|---|
| `unk_e1576aa8…` ações de VMT+0x98 | **resolvida** (revisão 3) |
| `unk_5ca0afe4…` modo de Texto | resolvida |
| `unk_7668c82c…` materialização Win32 | em investigação. A visibilidade padrão está resolvida. Faltam, em execução: ocultar menus vazios, `Visible`/`DynVisible`/`Enabled` e a personalização salva no perfil. |
| `unk_8535d836…` módulo por tipo | em investigação. Resolvido para 12 classes pelos getters VMT+0xc8 (`ev_cb64c79f…`). Tabela e XML não têm classe `TTable*`/`TXml*` no exe. |

Novos registros principais:

| Evidência | O que mostra |
|---|---|
| `ev_1197ea7d2195a8694e066e825a2ca3f8840fd57af17d5d98dbfcb1bfb643932d` | 0x1906760 `DisplayFilterModeClick` |
| `ev_81f8cde1e4c5fb84c48289bfd615057a62c92bc497206abf30964aeedfac34fd` | 0x1948f90 (ciclo de modos) e 0xfc5e30 |
| `ev_b2e60d18f09a0aebceaf2d82c3f75e341788173bfb85af52771e6e9a5ae28a2d` | 0x1947f20 (modos permitidos) e 0x19484a0 (visibilidade dos botões) |
| `ev_ec3a8aef6446f440668adeaea01a5c5f535ee4af07366f1b01665ebef90bd381` | 0x1947670, 0x1947c50, 0x19481a0, 0xfc50d0 |
| `ev_ef7bba5dcfbfc53adcf24b800df9ef4333ea3c106d92f257645ad467fb1d1c11` | 0x1947a10 (`MenuAllowed`/`ToolbarAllowed` → tabela) |
| `ev_cb64c79f0fa8cd0bde01bb831b514dc5fb784eafee9538e2e60268065726feb8` | getters VMT+0xc8 (subclasse → sessão) |
| `ev_db5aedb2ad653a31a85073298979db0ded21bff6c5d09ad975214b87f6010955` | strings `DropDown`/`Favorites`/`Toggles` |
