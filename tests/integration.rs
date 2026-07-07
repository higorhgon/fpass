//! Teste de integração: exercita o PassStore contra um password-store REAL.
//! Requer: pass, gpg com chave de teste, e PASSWORD_STORE_DIR/GNUPGHOME
//! apontando pro ambiente de teste. Roda com:
//!   cargo test --test integration -- --ignored --test-threads=1
#![allow(dead_code)] // métodos do store não exercitados pelo teste geram warning falso

use zeroize::Zeroizing;

// Importa os módulos via include! já que são binário, não lib.
// (alternativa simples a converter o projeto em lib+bin)
#[path = "../src/store.rs"]
mod store;

use store::{EntryData, PassStore};

fn test_store() -> PassStore {
    PassStore::open(None).expect("PASSWORD_STORE_DIR deve apontar pro store de teste")
}

fn passphrase() -> String {
    std::env::var("TEST_PASSPHRASE").unwrap_or_default()
}

#[test]
#[ignore]
fn integration_list_shows_entries_and_groups() {
    let s = test_store();
    let (entries, groups) = s.list();
    assert!(entries.contains(&"Trabalho/servidor-admin".to_string()));
    assert!(entries.contains(&"Pessoal/github".to_string()));
    assert!(groups.contains(&"Trabalho".to_string()));
    assert!(groups.contains(&"GrupoVazio".to_string()));
    // Nada de .gpg-id ou .git na listagem
    assert!(!entries.iter().any(|e| e.starts_with('.')));
}

#[test]
#[ignore]
fn integration_show_parses_fields() {
    let s = test_store();
    let d = s.show_with_passphrase("Trabalho/servidor-admin", &passphrase()).expect("show falhou");
    assert_eq!(d.password.as_str(), "s3nh4-admin");
    assert_eq!(d.username, "fulano");
    assert_eq!(d.url, "https://exemplo.com.br/admin");
}

#[test]
#[ignore]
fn integration_insert_edit_preserves_extra_and_mv_rm() {
    let s = test_store();

    // Insert novo
    let mut d = EntryData::default();
    d.password = Zeroizing::new("nova-senha".into());
    d.username = "user1".into();
    d.extra = vec!["notes: preservar isto".into()];
    s.insert("Teste/nova-entrada", &d).expect("insert falhou");

    // Edit: muda a senha, extra deve permanecer
    let mut read = s.show_with_passphrase("Teste/nova-entrada", &passphrase()).expect("show falhou");
    assert_eq!(read.extra, vec!["notes: preservar isto"]);
    read.password = Zeroizing::new("senha-editada".into());
    s.insert("Teste/nova-entrada", &read).expect("edit falhou");
    let read2 = s.show_with_passphrase("Teste/nova-entrada", &passphrase()).expect("show falhou");
    assert_eq!(read2.password.as_str(), "senha-editada");
    assert_eq!(read2.extra, vec!["notes: preservar isto"]);

    // Move
    s.mv("Teste/nova-entrada", "Teste/renomeada").expect("mv falhou");
    assert!(s.show_with_passphrase("Teste/renomeada", &passphrase()).is_ok());

    // Remove
    s.rm("Teste/renomeada").expect("rm falhou");
    assert!(s.show_with_passphrase("Teste/renomeada", &passphrase()).is_err());
}
