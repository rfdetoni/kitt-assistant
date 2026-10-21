# Assistant Runtime 0.4.36 — autonomia para checkout Git público

Atualiza o companion Python do Assistant para Agent CLI **0.88.47**, commit imutável `e2065939b5b798073130854c88b5ba281f672ffd`. Sincroniza `uv.lock`, a dependência `kitt-agent-cli>=0.88.47,<0.89` e os dois checkouts Agent do CI. Não altera Rust daemon, Cargo.lock ou HUD.

No modo autônomo, o Agent dispensa aprovação humana **somente** para `git clone` ou `git submodule add` HTTPS de repositórios públicos conhecidos com diretório de destino contido no workspace, principal root e sandbox de rede forte. Outros processos com rede e agentes delegados seguem submetidos aos controles existentes.

A instalação pela release principal do ecossistema exige verificação dos SHAs, testes Python e ciclo de vida nas três plataformas. O erro de modelos locais Ollama 404 e um eventual timeout longo de clone não são corrigidos nesta release.
