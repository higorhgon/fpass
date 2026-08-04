# fpass

[![CI](https://github.com/higorhgon/fpass/actions/workflows/ci.yml/badge.svg)](https://github.com/higorhgon/fpass/actions/workflows/ci.yml)

Gerenciador de senhas com interface TUI (Terminal User Interface), escrito em Rust. Suporta dois backends: **KeePassXC** (.kdbx, via `keepassxc-cli`) e **pass** — the standard unix password manager (via `gpg`).

## Funcionalidades

- Suporte a KeePassXC e a pass — o fpass detecta ambos automaticamente e adapta a interface a cada um (entradas do pass, por exemplo, têm só Título e Senha, sem Usuário/URL/Notas no formulário)
- No pass, a autenticação é feita pelo gpg-agent: o fpass nunca vê a senha mestra da chave GPG
- Seletor de banco de dados com busca multi-termo e navegação estilo vim
- Modal integrado para desbloqueio de banco com validação de senha/passphrase
- Criação de bancos pela própria interface: KeePassXC (nome + senha) ou pass (diretório com autocomplete + escolha de chave GPG existente)
- Suporte completo a mouse: clique/duplo-clique/botão direito na lista, seleção e cópia de texto nos modais
- Listagem, adição, edição, exclusão e renomeação de grupos/entradas
- Modal de ajuda com todos os atalhos (`Ctrl+?`)
- Motor de frecency (frequência + recência) para ordenação inteligente
- Sistema de temas personalizáveis em TOML (esquema de cores padrão: Catppuccin Mocha)
- Atalhos de teclado: `j/k`, `gg/G`, `Ctrl+U/D`, `/` para buscar

## Requisitos

- [Rust](https://rustup.rs/) (edition 2024)

Para bancos **KeePassXC**:

- [KeePassXC](https://keepassxc.org/) com `keepassxc-cli` disponível no PATH

Para bancos **pass**:

- [`pass`](https://www.passwordstore.org/) instalado
- `gpg`/`gpg-agent`, com pelo menos uma chave secreta já criada (veja [Configurando o pass](#configurando-o-pass))
- `gpg-agent` configurado para aceitar a senha via loopback (`allow-loopback-pinentry`) — necessário porque o fpass decifra entradas passando a senha pelo stdin do `gpg`, e o pinentry gráfico/curses padrão conflitaria com a própria TUI

## Instalação

```bash
git clone https://github.com/<usuario>/fpass.git
cd fpass
cargo build --release
```

## Uso

```bash
./target/release/fpass
```

O programa busca automaticamente, dentro do diretório configurado (`path` em `config.toml`, padrão: home directory):

- arquivos `.kdbx` (bancos KeePassXC)
- diretórios contendo `.gpg-id` (password-stores do pass)

e exibe uma interface interativa para seleção, desbloqueio e gestão de senhas, indicando o tipo de cada banco encontrado (`[KeePassXC]` ou `[pass]`).

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

**3. Permita que o gpg-agent aceite a senha via loopback**, para o fpass poder decifrar entradas sem abrir um pinentry gráfico/curses (que conflitaria com a TUI):

```bash
echo "allow-loopback-pinentry" >> ~/.gnupg/gpg-agent.conf
gpg-connect-agent reloadagent /bye
```

> **Nota:** no pass, o fpass só expõe **Título** e **Senha** no formulário de adicionar/editar — o formato do pass é bem menos estruturado que o do KeePassXC. Campos como usuário/URL/notas de uma entrada já existente são preservados ao editá-la, mesmo sem aparecer no formulário.

### Migrando um banco KeePassXC para o pass

```bash
fpass --kdbx2pass banco.kdbx KEYID1 [KEYID2...]
```

Inicializa (ou reaponta) o password-store padrão para os `KEYID`s informados e importa cada entrada do `.kdbx`, pedindo a senha mestra uma única vez. Username/URL/Notas viram metadados nas linhas seguintes à senha, no formato convencional do pass; nada é gravado em disco em texto puro durante o processo.

## Segurança

- **Backend KeePassXC**: a senha da entrada é sempre passada ao `keepassxc-cli` via stdin, mas `keepassxc-cli` não aceita usuário/URL/notas por stdin — esses campos vão como argumentos (`-u`, `--url`, `--notes`) em `add`/`edit`. Isso é uma limitação do `keepassxc-cli`, não do fpass: durante a execução do processo, outro usuário local com acesso a `/proc/<pid>/cmdline` (ou `ps aux`) pode ler esses valores. A senha em si nunca passa por argv. O backend **pass** não tem essa limitação — toda a entrada (senha e metadados) é enviada por stdin ao `gpg`/`pass insert`.
- **`~/.config/fpass/history`** guarda só hashes (com sal em `.history_key`, 0600) e timestamps de uso, nunca o conteúdo das entradas — mas ainda revela para outro usuário local com acesso ao arquivo quantas entradas existem e o padrão de uso.

## Configuração

Arquivos em `~/.config/fpass/`:

- `config.toml` — caminho de busca, recency, tema ativo e idioma
- `themes/*.toml` — definições de cores personalizadas

`path` define onde o fpass procura **tanto** arquivos `.kdbx` quanto password-stores do pass (diretórios com `.gpg-id`); o `~/.password-store` convencional é sempre verificado, mesmo que `path` aponte para outro lugar.

`language` controla o idioma da interface, em ordem de prioridade: (1) valor explícito no config.toml (`"pt-BR"`/`"en"`); (2) na ausência de um valor explícito (`"auto"` ou omitido), autodetecção pelo `$LANG`/`$LC_ALL` do sistema; (3) se nada foi configurado nem detectado, o padrão é inglês. Só o texto gerado pelo próprio fpass é traduzido — mensagens de erro que vêm direto do `keepassxc-cli`, `pass` ou `gpg` continuam no idioma dessas ferramentas, fora do controle do fpass.

Exemplo de `config.toml`:

```toml
[general]
path = "~/docs/keepass"
recency = true
theme = "meu-tema"
language = "pt-BR"
```

Exemplo de tema em `~/.config/fpass/themes/tema.toml`:

```toml
[theme]
name = "meu-tema"

[colors]
Title = "#00AAAA"
Base = "#CCCCCC"
Guidance = "#666666"
```

## Atalhos

Lista completa (mouse incluído) disponível a qualquer momento com `Ctrl+?`. Os mais essenciais:

| Tecla | Ação |
|-------|------|
| `j` / `k` | Navegar para baixo/cima |
| `gg` / `G` | Ir para o topo/final |
| `Ctrl+U` / `Ctrl+D` | Meia página para cima/baixo |
| `/` ou `f` | Entrar no modo de busca |
| `Enter` | Selecionar / Confirmar |
| `ESC` / `q` | Cancelar / Sair |
| `Ctrl+C` | Sair do programa |

## Dependências

- [ratatui](https://github.com/ratatui/ratatui) —-renderização TUI
- [crossterm](https://github.com/crossterm-rs/crossterm) — terminal backend
- [sha2](https://crates.io/crates/sha2) — hash SHA-256 para history
- [toml](https://crates.io/crates/toml) e [serde](https://crates.io/crates/serde) — configuração
