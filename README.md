# fpass

[![CI](https://github.com/higorhgon/fpass/actions/workflows/ci.yml/badge.svg)](https://github.com/higorhgon/fpass/actions/workflows/ci.yml)

Gerenciador de senhas com interface Qt Quick, escrito em C++, que segue o tema
do Omarchy e o modo claro/escuro do sistema automaticamente. Suporta dois
backends: **KeePassXC** (.kdbx, via `keepassxc-cli`) e **pass** — the standard
unix password manager (via `gpg`).

Feito na mesma linha do [omacalc](https://github.com/omacom-io/omacalc),
[omawrite](https://github.com/omacom-io/omawrite) e
[omacut](https://github.com/omacom-io/omacut): Qt 6 + QML, cores lidas do tema
ativo do Omarchy e retintadas ao vivo quando o tema muda, tipografia seguindo o
tamanho de texto do desktop.

## Funcionalidades

- Suporte a KeePassXC e a pass — o fpass detecta ambos automaticamente e adapta a interface a cada um (entradas do pass, por exemplo, têm só Título e Senha, sem Usuário/URL/Notas no formulário)
- No pass, o fpass guarda a passphrase só durante a sessão e decifra as entradas com ela
- Seletor de banco de dados com busca multi-termo e navegação estilo vim
- Modal integrado para desbloqueio de banco com validação de senha/passphrase
- Criação de bancos pela própria interface: KeePassXC (nome + senha) ou pass (diretório com autocomplete + escolha de chave GPG existente)
- Listagem, adição, edição, exclusão e renomeação de grupos/entradas
- Modal de ajuda com todos os atalhos (`Ctrl+?`)
- Motor de frecency (frequência + recência) para ordenação inteligente
- Cores vindas do tema do Omarchy, com sobreposição opcional por tema próprio em TOML
- Atalhos de teclado: `j/k`, `gg/G`, `Ctrl+U/D`, `/` para buscar — os mesmos da versão TUI
- Suporte a mouse: clique, duplo-clique e botão direito na lista; seleção de texto nos detalhes da entrada

## Requisitos

- Qt 6: `qt6-base`, `qt6-declarative`
- `xdg-desktop-portal` e um backend de portal (para o modo claro/escuro e o tamanho de texto do desktop)
- `wl-clipboard` (`wl-copy`) para copiar senhas

Para bancos **KeePassXC**:

- [KeePassXC](https://keepassxc.org/) com `keepassxc-cli` disponível no PATH

Para bancos **pass**:

- [`pass`](https://www.passwordstore.org/) instalado
- `gpg`/`gpg-agent`, com pelo menos uma chave secreta já criada (veja [Configurando o pass](#configurando-o-pass))
- `gpg-agent` configurado para aceitar a senha via loopback (`allow-loopback-pinentry`) — necessário porque o fpass decifra entradas passando a passphrase pelo stdin do `gpg`, sem abrir um pinentry a cada acesso

A busca por bancos usa [`fd`](https://github.com/sharkdp/fd) quando disponível
(bem mais rápido em um diretório home inteiro) e cai para uma varredura própria
quando não está instalado.

A fonte iA Writer Mono é distribuída junto sob a SIL Open Font License 1.1; veja
`fonts/OFL.txt`. A fonte é copyright Information Architects Inc. e derivada da
IBM Plex, copyright IBM Corp.

## Instalação

```bash
git clone https://github.com/<usuario>/fpass.git
cd fpass
./bin/build
```

O binário fica em `build/fpass`. Os testes rodam com `./bin/test`.

## Uso

```bash
./build/fpass
```

O programa busca automaticamente, dentro do diretório configurado (`path` em
`config.toml`, padrão: home directory):

- arquivos `.kdbx` (bancos KeePassXC)
- diretórios contendo `.gpg-id` (password-stores do pass)

e exibe uma interface interativa para seleção, desbloqueio e gestão de senhas,
indicando o tipo de cada banco encontrado (`[KeePassXC]` ou `[pass]`). Havendo
um único banco, ele vai direto para o modal de desbloqueio.

### Criando um banco pela interface

Na tela de seleção, `Ctrl+A` abre um menu perguntando o tipo de banco a criar:

- **KeePassXC** — pede nome do arquivo e senha mestra; o banco é criado em `~/.config/fpass/databases/`.
- **pass** — pede o diretório de destino (com autocomplete dos nomes de pasta existentes) e uma chave GPG dentre as já presentes no seu chaveiro. O fpass não gera chaves GPG novas — veja a seção abaixo para criar uma.

### Configurando o pass

Se preferir configurar por fora da interface (ou ainda não tiver uma chave GPG):

**1. Gere uma chave GPG**, caso ainda não tenha uma:

```bash
gpg --full-generate-key
```

Siga os prompts (tipo e tamanho da chave, validade, nome, e-mail e senha). Para conferir as chaves disponíveis depois:

```bash
gpg --list-secret-keys --keyid-format long
```

**2. Inicialize um password-store** com essa chave:

```bash
pass init <SEU-GPG-KEY-ID>
```

Isso cria o store em `~/.password-store`. Para criar em outro lugar, defina `PASSWORD_STORE_DIR` antes de rodar o comando:

```bash
PASSWORD_STORE_DIR=/caminho/personalizado pass init <SEU-GPG-KEY-ID>
```

O fpass encontra automaticamente qualquer diretório com `.gpg-id` dentro do seu diretório de busca configurado (veja `path` em [Configuração](#configuração)) — é possível ter múltiplos password-stores em locais diferentes.

**3. Permita que o gpg-agent aceite a senha via loopback**, para o fpass poder decifrar entradas com a passphrase que já pediu uma vez:

```bash
echo "allow-loopback-pinentry" >> ~/.gnupg/gpg-agent.conf
gpg-connect-agent reloadagent /bye
```

> **Nota:** no pass, o fpass só expõe **Título** e **Senha** no formulário de adicionar/editar — o formato do pass é bem menos estruturado que o do KeePassXC. Campos como usuário/URL/notas de uma entrada já existente são preservados ao editá-la, mesmo sem aparecer no formulário.

### Migrando um banco KeePassXC para o pass

```bash
fpass --kdbx2pass banco.kdbx KEYID1 [KEYID2...]
```

Roda no terminal, sem abrir janela. Inicializa (ou reaponta) o password-store
padrão para os `KEYID`s informados e importa cada entrada do `.kdbx`, pedindo a
senha mestra uma única vez. Username/URL/Notas viram metadados nas linhas
seguintes à senha, no formato convencional do pass; nada é gravado em disco em
texto puro durante o processo.

## Segurança

- **Backend KeePassXC**: a senha da entrada é sempre passada ao `keepassxc-cli` via stdin, mas `keepassxc-cli` não aceita usuário/URL/notas por stdin — esses campos vão como argumentos (`-u`, `--url`, `--notes`) em `add`/`edit`. Isso é uma limitação do `keepassxc-cli`, não do fpass: durante a execução do processo, outro usuário local com acesso a `/proc/<pid>/cmdline` (ou `ps aux`) pode ler esses valores. A senha em si nunca passa por argv. O backend **pass** não tem essa limitação — toda a entrada (senha e metadados) é enviada por stdin ao `gpg`/`pass insert`.
- Senhas e passphrases circulam em um tipo `Secret`, que mantém uma cópia própria e sobrescreve a memória ao ser destruído. A exceção inevitável é o campo de senha do formulário de edição: um campo editável precisa do texto em claro enquanto está na tela.
- Ao copiar uma senha, o conteúdo é marcado como sensível para o `wl-clipboard` (mime `x-kde-passwordManagerHint`, que gerenciadores como o cliphist respeitam para não gravar no histórico) e o clipboard é limpo automaticamente após 10 segundos, com contagem regressiva visível na interface.
- **`~/.config/fpass/history`** guarda só HMACs (com chave aleatória local em `.history_key`, 0600) e timestamps de uso, nunca o conteúdo das entradas — mas ainda revela para outro usuário local com acesso ao arquivo quantas entradas existem e o padrão de uso.

## Configuração

Arquivos em `~/.config/fpass/`:

- `config.toml` — caminho de busca, recency, tema ativo e idioma
- `themes/*.toml` — sobreposições de cores

`path` define onde o fpass procura **tanto** arquivos `.kdbx` quanto password-stores do pass (diretórios com `.gpg-id`); o `~/.password-store` convencional é sempre verificado, mesmo que `path` aponte para outro lugar.

`language` controla o idioma da interface, em ordem de prioridade: (1) valor explícito no config.toml (`"pt-BR"`/`"en"`); (2) na ausência de um valor explícito (`"auto"` ou omitido), autodetecção pelo `$LANG`/`$LC_ALL` do sistema; (3) se nada foi configurado nem detectado, o padrão é inglês. Só o texto gerado pelo próprio fpass é traduzido — mensagens de erro que vêm direto do `keepassxc-cli`, `pass` ou `gpg` continuam no idioma dessas ferramentas, fora do controle do fpass.

Exemplo de `config.toml`:

```toml
[general]
path = "~/docs/keepass"
recency = true
theme = "default"
language = "pt-BR"
```

### Cores

Por padrão (`theme = "default"`) as cores vêm do tema ativo do Omarchy, em
`~/.local/state/omarchy/current/theme/colors.toml`, e são reaplicadas na hora
quando você troca de tema — não é preciso reiniciar o fpass. Sem Omarchy
instalado, o fpass usa Catppuccin Mocha ou Latte conforme a preferência
clara/escura do desktop.

Um tema próprio sobrepõe apenas os papéis que ele declarar; o resto continua
vindo do Omarchy. Em `~/.config/fpass/themes/tema.toml`:

```toml
[theme]
name = "meu-tema"

[colors]
Title = "#00AAAA"
Base = "#CCCCCC"
Guidance = "#666666"
```

e então `theme = "meu-tema"` no `config.toml`. Os papéis disponíveis são
`Title`, `Base`, `Guidance`, `Annotation`, `Important`, `AlertInfo`,
`AlertWarn` e `AlertError`.

O tamanho do texto segue o do desktop — `omarchy display text size`, ou o
`text-scaling-factor` do GNOME.

## Atalhos

Lista completa disponível a qualquer momento com `Ctrl+?`. Os mais essenciais:

| Tecla | Ação |
|-------|------|
| `j` / `k` | Navegar para baixo/cima |
| `gg` / `G` | Ir para o topo/final |
| `Ctrl+U` / `Ctrl+D` | Meia página para cima/baixo |
| `Ctrl+N` / `Ctrl+P` | Próximo/anterior (funciona na busca e nas dropdowns) |
| `/`, `f` ou `i` | Entrar no modo de busca |
| `Enter` | Copiar senha / Confirmar |
| `Tab` | Ver detalhes da entrada |
| `Espaço` | Menu de ações |
| `Ctrl+A` / `Ctrl+E` / `Ctrl+X` | Adicionar / editar / excluir |
| `ESC` / `q` | Cancelar / Sair |
| `Ctrl+Q` | Sair do programa |
| `Ctrl+C` | Sair do programa (fora de campos de texto, onde copia) |
