# fpass 2.x — TUI para o `pass`

TUI rápida com navegação vim para o [pass](https://www.passwordstore.org/)
(the standard unix password manager). Sucessora do fpass 1.x, que usava
`keepassxc-cli` como backend.

## Por que a migração para o pass?

| | fpass 1.x (keepassxc-cli) | fpass 2.x (pass) |
|---|---|---|
| Senha mestra em memória | `String` viva o tempo todo | **Nunca entra no processo** — gpg-agent/pinentry cuida |
| Dependência externa | KeePassXC inteiro (Qt5 etc.) | `pass` + `gpg` (infra padrão de distro) |
| Compartilhamento em equipe | senha mestra compartilhada | multi-destinatário GPG (`pass init KEY1 KEY2 ...`) |
| Histórico de alterações | não | `pass git` (cada mudança é um commit) |
| Latência por operação | ~1s (CLI reabre o banco) | ms (leitura direta do filesystem p/ listagem) |

## Requisitos

- `pass` instalado e store inicializado: `pass init <SEU-GPG-KEY-ID>`
- `gpg-agent` com **pinentry gráfico** (`pinentry-gnome3`, `pinentry-qt`...).
  `pinentry-curses` disputa o terminal com a TUI — configure em
  `~/.gnupg/gpg-agent.conf`: `pinentry-program /usr/bin/pinentry-gnome3`
- Wayland: `wl-clipboard` (e opcionalmente `cliphist` para limpeza do histórico)

## Configuração

`~/.config/fpass/config.toml`:

```toml
[general]
# store = "~/.password-store"   # default: $PASSWORD_STORE_DIR ou ~/.password-store
recency = true                  # ordenação por frecency
theme = "default"               # nome de um tema em ~/.config/fpass/themes/
clip_time = 10                  # segundos até limpar o clipboard
```

Temas: mesmos arquivos TOML do fpass 1.x em `~/.config/fpass/themes/`.

## Atalhos

Mesmos do 1.x: `/` ou `f` pesquisa, `j/k` navega, `gg/G` topo/fim,
`CTRL-U/D` meia página, `ENTER` copia a senha, `CTRL-A` adiciona,
`CTRL-E` edita, `CTRL-X` exclui, `q`/`ESC` sai.

## Segurança — o que mudou

1. **Senha mestra fora do processo.** A autenticação é do gpg-agent; o fpass
   nunca lê, armazena ou trafega a passphrase.
2. **Segredos zerados na memória.** Senhas de entradas usam
   `zeroize::Zeroizing<String>` — zeradas ao sair de escopo (cópia, forms,
   thread de limpeza do clipboard).
3. **Clipboard limpo de verdade.** `wl-copy --clear` (a flag correta) em vez
   de escrever string vazia; `cliphist delete-query` continua sendo chamado.
4. **Frecency com HMAC.** O histórico grava HMAC-SHA256(chave_local, entrada)
   com chave aleatória de 32 bytes em `~/.config/fpass/history.key` (0600) —
   sem rainbow table possível, diferente do SHA-256 puro do 1.x.
5. **Sem `unwrap()` em caminhos críticos.** Falhas do `pass`/clipboard viram
   mensagens de erro na TUI, não panics.
6. **Edição preserva campos desconhecidos.** Linhas extras da entrada
   (`otp:`, `notes:`, campos de outras ferramentas) são mantidas no edit.

## Arquitetura

```
src/
├── main.rs      # bootstrap, args, loop de eventos
├── config.rs    # config + temas TOML
├── history.rs   # frecency (HMAC-SHA256)
├── store.rs     # wrapper do pass (run_pass único p/ todas operações)
├── clipboard.rs # cópia + limpeza agendada
├── app.rs       # estado e lógica (sem UI)
└── ui.rs        # desenho ratatui (sem lógica)
tests/
└── integration.rs  # testes contra um password-store real
```

## Testes

```sh
cargo test                                            # 15 testes unitários
# Integração (precisa de um store de teste em $PASSWORD_STORE_DIR):
cargo test --test integration -- --ignored --test-threads=1
```

## Migrando do fpass 1.x (kdbx)

Use o script `kdbx2pass.sh` (conversão via `keepassxc-cli export`) e depois
remova o KeePassXC. Seu histórico de frecency antigo não é migrável (os hashes
mudaram de SHA-256 para HMAC com chave nova) — ele se reconstrói com o uso.
