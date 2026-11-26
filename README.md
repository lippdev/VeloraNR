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

## Git Flow

- `main` contém versões publicadas. `develop` recebe a integração.
- Abra branches `feature/<nome>` a partir de `develop` e PRs de volta para `develop`.
- Prepare versões em `release/<versão>` e correções urgentes em `hotfix/<nome>` a partir de `main`. Integre ambas em `main` e `develop`.
- Use commits convencionais, como `feat(discovery): detect consoles on the LAN`. Os PRs incluem resumo, mudanças, verificação e documentação.
- Antes do push, aplique a skill [ship do LippStack](https://github.com/lippdev/lippstack) para revisar o diff, executar os testes e validar o build.
