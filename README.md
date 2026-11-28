# VeloraNR

MVP de Remote Play do PS5 para Windows x64. A interface é Rust + GPUI. O app busca consoles na rede, faz o pareamento na própria janela e aplica os ajustes de qualidade via D3D12. O perfil do console é criptografado pelo Windows e fica em %LOCALAPPDATA%\PortalPS5.

## Build e pacote local

A interface precisa de Rust MSVC, do Windows SDK para compilar o ícone e de `portal_media.lib`. Defina `PORTAL_NATIVE_DIR` com o diretório que contém essa biblioteca e a DLL correspondente. Execute `cargo build --locked` e, com a DLL e as dependências acessíveis pelo PATH, `cargo test --locked`.

```powershell
./package.ps1 -NativeBuild 'C:/build/portal-native' -RuntimeAssets 'C:/build/media-assets'
```

O executável fica em `dist/veloranr.exe`. Mantenha as DLLs, os shaders e os modelos ao lado dele na estrutura criada pelo script.

A camada nativa atual usa Chiaki, FFmpeg, SDL e o motor D3D12 em C++. O repositório publica o código do MVP e a ponte `media.cpp`. Para recompilar a ponte, o ambiente de origem precisa fornecer `veyra_engine`, `chiaki-lib`, `SDL3::SDL3-static` e `src/remoteplay/ControllerInput.cpp`. O checkout público ainda depende desse build nativo externo. As DLLs, os modelos e os runtimes são fornecidos localmente. Inclua `runtime/config/ngx-local.json` para o DLSS NR e os runtimes de geração de quadros correspondentes ao pacote local.

O Remote Play foi verificado com um PS5 já vinculado. A sessão confirmou DLSS NR antes do upscaling, MLAA, XeSS FG 2× e NVIDIA VFG 2× e 4×. O primeiro vínculo com um novo código e o despertar do console em repouso ainda precisam de validação ao vivo.

Na aba **DLSS 5**, escolha a DLL otimizada para RTX 20, 30 e 40 ou a original do pacote, recomendada para RTX 50. O empacotador espera o arquivo local `nvngx_dlssnr.dll` nas pastas `runtime/experimental/nr-ampere` e `runtime/experimental/nr-original` de RuntimeAssets.

Em **Frame Generation → Captura Next Boost → Contador de FPS**, o modo detalhado usa o contador do motor. **Somente FPS final** mostra a taxa de saída medida pelo Next Boost, atualizada durante a transmissão.

## Frame Generation por captura (Next Boost)

Para incluir o motor e os pacotes NVIDIA e Intel instalados nessa pasta no pacote local, passe também `-SuperMotionEngine 'C:/caminho/framegen/engine'` ao script. O VeloraNR usa sua própria cópia do núcleo. Na aba **Frame Generation**, selecione **Captura Next Boost**, o gerador **Super Motion (SMFG)**, **NVIDIA DLSS MFG** ou **Intel XeSS MFG** e o multiplicador de 2× a 6×. Durante o jogo, F11 alterna a tela cheia sem bordas. Encoste o mouse na extremidade esquerda do player para abrir **Ajustes rápidos** e volte ao vídeo para recolher o painel. Em **Frame Gen**, escolha **Desligado** ou um multiplicador. O mesmo painel permite ajustar DLSS 5, antisserrilhamento e super-resolução enquanto a sessão continua. Ao sair da tela de streaming, a captura para. O contador informa as taxas observadas pelo motor. O DLSS NR continua sendo configurado na aba **DLSS 5**.

Para testar sem console, execute `veloranr.exe --test-video "C:/videos/teste.mp4"`. O vídeo passa pelo mesmo renderer e pelos mesmos controles de qualidade, com preferências e log separados em `%LOCALAPPDATA%/VeloraNR-test-video`.

## Fluidez e diagnóstico

Em **Ajustes rápidos**, **Medir cadência · 15 s** registra os intervalos entre apresentações submetidas ao kernel gráfico pelo renderer e pelo Super Motion. A captura usa ETW do Windows. O relatório preserva os eventos dos processos do VeloraNR e do motor. Uma medição com perdas é identificada como parcial. Se o Windows recusar a permissão de rastreamento, o painel informa e o streaming continua.

**Abrir relatório** abre a pasta da medição. `apresentacoes.csv` preserva o horário UTC em microssegundos Unix e o intervalo de cada chamada. `resumo.csv` agrupa mediana, P95 e máximo por processo e janela de destino. O campo `videoHwnd` identifica a superfície de vídeo entre as janelas do renderer. `sessao.log` usa UTC e registra a superfície de vídeo, ajustes, confirmações, FPS do motor e tempos de processamento. Durante Remote Play, `ratesReady=true` identifica amostras válidas de recepção e decodificação. A medição descreve submissões ao Windows; medir a latência do controle e a exibição física requer instrumentação separada.

## Git Flow

- `main` contém versões publicadas. `develop` recebe a integração.
- Abra branches `feature/<nome>` a partir de `develop` e PRs de volta para `develop`.
- Prepare versões em `release/<versão>` e correções urgentes em `hotfix/<nome>` a partir de `main`. Integre ambas em `main` e `develop`.
- Use commits convencionais, como `feat(discovery): detect consoles on the LAN`. Os PRs incluem resumo, mudanças, verificação e documentação.
- Antes do push, aplique a skill [ship do LippStack](https://github.com/lippdev/lippstack) para revisar o diff, executar os testes e validar o build.
