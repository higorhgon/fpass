fn main() {
    // A macro `i18n!()` do rust-i18n lê `locales/` em tempo de expansão, mas
    // não registra esses arquivos como dependência de rebuild (não usa
    // `tracked_path`). Sem isso, `cargo build` não recompila depois de só
    // editar um .yml de tradução.
    println!("cargo:rerun-if-changed=locales");
}
