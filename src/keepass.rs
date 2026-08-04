use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

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

