use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};
use zeroize::Zeroizing;

/// Resultado da execução de um comando `keepassxc-cli`.
pub struct KpOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Executa `keepassxc-cli` com os argumentos dados, escrevendo cada item de
/// `stdin_lines` (ex.: senha do banco, nova senha, confirmação) como uma
/// linha no stdin do processo. Centraliza o padrão spawn/stdin/wait usado em
/// todo o app para evitar repetição e permitir tratamento de erro uniforme.
pub fn run_kpcli(args: &[&str], stdin_lines: &[&str]) -> Result<KpOutput, String> {
    let mut cmd = Command::new("keepassxc-cli");
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("Não foi possível executar keepassxc-cli: {}", e))?;

    if let Some(mut stdin) = child.stdin.take() {
        for line in stdin_lines {
            let _ = stdin.write_all(line.as_bytes());
            let _ = stdin.write_all(b"\n");
        }
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("Falha ao aguardar keepassxc-cli: {}", e))?;

    Ok(KpOutput {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

pub fn find_databases(path: &str) -> Vec<String> {
    let mut dbs = Vec::new();

    if let Ok(output) = Command::new("fd").args([".kdbx$", path]).output() {
        dbs.extend(String::from_utf8_lossy(&output.stdout).lines().map(|s| s.trim()).filter(|s| !s.is_empty()).map(String::from));
    }

    let home = std::env::var("HOME").unwrap_or_default();
    let fpass_db_dir = format!("{}/.config/fpass/databases", home);
    if let Ok(output) = Command::new("fd").args([".kdbx$", &fpass_db_dir]).output() {
        dbs.extend(String::from_utf8_lossy(&output.stdout).lines().map(|s| s.trim()).filter(|s| !s.is_empty()).map(String::from));
    }

    dbs.sort();
    dbs.dedup();
    dbs
}

pub fn create_database(name: &str, password: &str) -> Result<PathBuf, String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let db_dir = PathBuf::from(format!("{}/.config/fpass/databases", home));
    fs::create_dir_all(&db_dir).map_err(|e| format!("Erro ao criar diretório: {}", e))?;

    let db_path = db_dir.join(format!("{}.kdbx", name));
    if db_path.exists() {
        return Err("Arquivo já existe!".to_string());
    }

    let db_path_str = db_path
        .to_str()
        .ok_or_else(|| "Caminho do banco contém caracteres inválidos".to_string())?;

    let result = run_kpcli(&["db-create", "-p", db_path_str], &[password, password])?;
    if result.success {
        Ok(db_path)
    } else {
        Err(result.stderr)
    }
}

fn clipboard_cmd(is_mac: bool) -> &'static str {
    if is_mac { "pbcopy" } else { "wl-copy" }
}

pub fn copy_to_clipboard(text: &str, is_mac: bool) -> Result<(), String> {
    let mut cmd = Command::new(clipboard_cmd(is_mac));
    if !is_mac {
        // Sinaliza ao wl-clipboard (>=2.2.1) que o conteúdo é sensível: ele expõe
        // o mime type `x-kde-passwordManagerHint`, que gerenciadores de clipboard
        // como o cliphist reconhecem e usam para NÃO persistir o valor no
        // histórico em disco. Isso resolve o problema na origem, em vez de tentar
        // apagar a senha do histórico depois que ela já foi gravada.
        cmd.arg("--sensitive");
    }
    let mut child = cmd.stdin(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(text.as_bytes());
    }
    child.wait().map(|_| ()).map_err(|e| e.to_string())
}

/// Limpa o clipboard. No Wayland, `wl-copy --clear` avisa explicitamente o
/// compositor para descartar a seleção — ao contrário de simplesmente
/// escrever uma string vazia via stdin, que alguns compositores não tratam
/// como uma nova seleção válida e por isso preservam o valor anterior.
fn clear_clipboard(is_mac: bool) {
    if is_mac {
        if let Ok(mut child) = Command::new("pbcopy").stdin(Stdio::piped()).spawn() {
            if let Some(mut stdin) = child.stdin.take() { let _ = stdin.write_all(b""); }
            let _ = child.wait();
        }
    } else {
        let _ = Command::new("wl-copy").arg("--clear").status();
    }
}

pub fn spawn_clipboard_clearer(password: Zeroizing<String>, is_mac: bool) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(10));
        clear_clipboard(is_mac);
        if !is_mac {
            // Fallback para wl-clipboard < 2.2.1 / cliphist sem suporte ao hint
            // `--sensitive` (ver copy_to_clipboard): tenta apagar a entrada do
            // histórico persistido. Sem efeito (e sem erro) se o cliphist não
            // estiver instalado ou se o hint já tiver evitado o armazenamento.
            let _ = Command::new("cliphist").args(["delete-query", password.as_str()]).status();
        }
        // `password` sai de escopo aqui e o Zeroizing sobrescreve a memória.
    });
}
