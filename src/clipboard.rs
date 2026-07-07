use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;
use zeroize::Zeroizing;

enum Backend {
    Mac,
    Wayland,
    X11,
}

fn detect(is_mac: bool) -> Backend {
    if is_mac {
        return Backend::Mac;
    }
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        Backend::Wayland
    } else {
        Backend::X11
    }
}

pub fn copy_to_clipboard(secret: &Zeroizing<String>, is_mac: bool) -> Result<(), String> {
    match detect(is_mac) {
        Backend::Mac => pipe_to("pbcopy", &[], secret),
        // --sensitive adiciona x-kde-passwordManagerHint aos MIME types,
        // fazendo o Elephant ignorar o conteúdo (não armazena no histórico)
        Backend::Wayland => pipe_to("wl-copy", &["--sensitive"], secret),
        Backend::X11 => pipe_to("xclip", &["-selection", "clipboard"], secret),
    }
}

fn pipe_to(cmd: &str, args: &[&str], data: &Zeroizing<String>) -> Result<(), String> {
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Erro executando {}: {}", cmd, e))?;

    let mut stdin = child.stdin.take().ok_or_else(|| "stdin indisponível".to_string())?;
    stdin
        .write_all(data.as_bytes())
        .map_err(|e| format!("Erro copiando: {}", e))?;
    drop(stdin);

    child
        .wait()
        .map_err(|e| format!("Erro no {}: {}", cmd, e))?;
    Ok(())
}

pub fn spawn_clipboard_clearer(secret: Zeroizing<String>, is_mac: bool, secs: u64) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(secs));

        match detect(is_mac) {
            Backend::Mac => {
                if let Ok(mut child) = Command::new("pbcopy").stdin(Stdio::piped()).spawn() {
                    drop(child.stdin.take());
                    let _ = child.wait();
                }
            }
            Backend::Wayland => {
                let _ = Command::new("wl-copy")
                    .arg("--clear")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                let _ = Command::new("cliphist")
                    .args(["delete-query", secret.as_str()])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
            Backend::X11 => {
                if let Ok(mut child) = Command::new("xclip")
                    .args(["-selection", "clipboard"])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                {
                    drop(child.stdin.take());
                    let _ = child.wait();
                }
            }
        }
    });
}
