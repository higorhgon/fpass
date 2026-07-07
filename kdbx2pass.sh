#!/usr/bin/env bash
set -euo pipefail

DB="$1"                    # caminho do arquivo .kdbx
shift
RECIPIENTS=("$@")          # key IDs GPG dos membros da equipe

if [ -z "${DB:-}" ] || [ ${#RECIPIENTS[@]} -eq 0 ]; then
  echo "Uso: $0 banco.kdbx KEYID1 KEYID2 ..." >&2
  exit 1
fi

# 1. Garante que o password-store está inicializado pros destinatários certos
pass init "${RECIPIENTS[@]}"

# 2. Exporta o kdbx pra CSV (pede a senha mestra do kdbx uma vez)
CSV_TMP=$(mktemp)
trap 'shred -u "$CSV_TMP" 2>/dev/null || rm -f "$CSV_TMP"' EXIT
keepassxc-cli export --format csv "$DB" > "$CSV_TMP"

# 3. Processa o CSV com Python (csv module lida com aspas/vírgulas com segurança)
python3 - "$CSV_TMP" <<'PYEOF'
import csv, subprocess, sys, re

path = sys.argv[1]

# Nomes de grupo raiz que o KeePassXC costuma usar/gerar.
# Entradas nesses grupos vão direto pra raiz do password-store,
# sem criar uma subpasta com esse nome.
ROOT_GROUP_NAMES = {"", "Principal", "Root", "/"}

with open(path, newline='', encoding='utf-8') as f:
    reader = csv.DictReader(f)
    for row in reader:
        raw_group = row.get("Group", "").strip("/").replace("/./", "/")
        title = row.get("Title", "sem-titulo").strip()
        username = row.get("Username", "")
        password = row.get("Password", "")
        url = row.get("URL", "")
        notes = row.get("Notes", "")

        # sanitiza o path (sem espaços/caracteres esquisitos no nome do arquivo)
        safe_title = re.sub(r'[^\w\-. ]', '_', title).strip() or "sem-titulo"

        # Remove o primeiro segmento do caminho se for um nome de grupo raiz
        # conhecido (ex: "Principal/Trabalho" -> "Trabalho", "Principal" -> "")
        segments = raw_group.split("/") if raw_group else []
        if segments and segments[0] in ROOT_GROUP_NAMES:
            segments = segments[1:]
        group = "/".join(segments)

        if not group:
            entry_path = safe_title
        else:
            entry_path = f"{group}/{safe_title}"

        # monta o conteúdo multi-linha no formato que o pass espera:
        # linha 1 = senha, linhas seguintes = metadados
        lines = [password]
        if username:
            lines.append(f"username: {username}")
        if url:
            lines.append(f"url: {url}")
        if notes:
            lines.append(f"notes: {notes}")
        content = "\n".join(lines)

        print(f"Importando: {entry_path}")
        subprocess.run(
            ["pass", "insert", "-m", "-f", entry_path],
            input=content, text=True, check=True
        )
PYEOF

echo "Conversão concluída. Revise com: pass"
