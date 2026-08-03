use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

type HmacSha256 = Hmac<Sha256>;

/// Motor de frecency: guarda, para cada item usado, um contador de uso e o
/// timestamp do último uso, para ordenar os resultados por relevância.
///
/// Os paths das entradas nunca são gravados em claro no arquivo de
/// histórico: usamos HMAC-SHA256 com uma chave aleatória local (gerada uma
/// vez e guardada em `~/.config/fpass/.history_key`, 0600) em vez de um
/// SHA-256 puro, para não ficar vulnerável a rainbow tables caso alguém
/// tente reverter paths conhecidos de entradas KeePass.
pub struct History {
    records: HashMap<String, (u32, u64)>,
    file_path: PathBuf,
    enabled: bool,
    key: Zeroizing<Vec<u8>>,
}

impl History {
    pub fn new(enabled: bool) -> Self {
        let home = std::env::var("HOME").unwrap_or_default();
        let config_dir = PathBuf::from(format!("{}/.config/fpass", home));
        let file_path = config_dir.join("history");
        let key_path = config_dir.join(".history_key");
        let mut records = HashMap::new();
        let key = Zeroizing::new(load_or_create_key(&key_path));

        if enabled {
            if let Ok(file) = fs::File::open(&file_path) {
                for line in BufReader::new(file).lines().flatten() {
                    let parts: Vec<&str> = line.splitn(3, '|').collect();
                    if parts.len() == 3 {
                        if let (Ok(count), Ok(ts)) = (parts[1].parse(), parts[2].parse()) {
                            records.insert(parts[0].to_string(), (count, ts));
                        }
                    }
                }
            }
        }
        Self { records, file_path, enabled, key }
    }

    fn hash_item(&self, item: &str) -> String {
        let mut mac = HmacSha256::new_from_slice(&self.key).expect("HMAC aceita chave de qualquer tamanho");
        mac.update(item.as_bytes());
        mac.finalize().into_bytes().iter().map(|b| format!("{:02x}", b)).collect()
    }

    pub fn record_use(&mut self, item: &str) {
        if !self.enabled { return; }
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let hashed_item = self.hash_item(item);
        let entry = self.records.entry(hashed_item).or_insert((0, 0));
        entry.0 += 1; entry.1 = now;
        self.save();
    }

    fn save(&self) {
        if !self.enabled { return; }
        if let Ok(mut file) = OpenOptions::new().write(true).create(true).truncate(true).open(&self.file_path) {
            for (hash, (count, ts)) in &self.records { let _ = writeln!(file, "{}|{}|{}", hash, count, ts); }
        }
    }

    pub fn get_score(&self, item: &str) -> u64 {
        if !self.enabled { return 0; }
        let hashed_item = self.hash_item(item);
        if let Some(&(count, ts)) = self.records.get(&hashed_item) {
            let age = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs().saturating_sub(ts);
            (count as u64) * weight_for_age(age)
        } else { 0 }
    }

    pub fn sort_items(&self, items: &mut [String]) {
        if self.enabled {
            items.sort_by(|a, b| {
                let score_a = self.get_score(a);
                let score_b = self.get_score(b);
                score_b.cmp(&score_a).then_with(|| a.cmp(b))
            });
        }
    }
}

/// Peso de frecency de acordo com a idade (em segundos) do último uso.
/// Função pura para facilitar testes isolados do algoritmo de scoring.
fn weight_for_age(age_secs: u64) -> u64 {
    if age_secs < 86_400 { 100 }
    else if age_secs < 604_800 { 50 }
    else if age_secs < 2_592_000 { 20 }
    else { 5 }
}

fn load_or_create_key(key_path: &PathBuf) -> Vec<u8> {
    if let Ok(mut existing) = fs::File::open(key_path) {
        let mut key = Vec::new();
        if existing.read_to_end(&mut key).is_ok() && key.len() == 32 {
            return key;
        }
    }

    let key = random_bytes_32();
    if let Ok(mut f) = fs::File::create(key_path) {
        let _ = f.write_all(&key);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = f.set_permissions(fs::Permissions::from_mode(0o600));
        }
    }
    key
}

fn random_bytes_32() -> Vec<u8> {
    let mut key = vec![0u8; 32];
    if let Ok(mut urandom) = fs::File::open("/dev/urandom") {
        if urandom.read_exact(&mut key).is_ok() {
            return key;
        }
    }
    // Fallback extremamente improvável (sistema sem /dev/urandom): deriva
    // bytes a partir do relógio para nunca deixar a chave zerada/previsível.
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    for (i, b) in key.iter_mut().enumerate() {
        *b = ((nanos >> ((i % 16) * 8)) & 0xff) as u8;
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weight_for_age_boundaries() {
        assert_eq!(weight_for_age(0), 100);
        assert_eq!(weight_for_age(86_399), 100);
        assert_eq!(weight_for_age(86_400), 50);
        assert_eq!(weight_for_age(604_799), 50);
        assert_eq!(weight_for_age(604_800), 20);
        assert_eq!(weight_for_age(2_591_999), 20);
        assert_eq!(weight_for_age(2_592_000), 5);
        assert_eq!(weight_for_age(u64::MAX), 5);
    }

    fn test_history() -> History {
        History {
            records: HashMap::new(),
            file_path: PathBuf::from("/dev/null"),
            enabled: true,
            key: Zeroizing::new(vec![42u8; 32]),
        }
    }

    #[test]
    fn hash_item_is_deterministic_for_same_key() {
        let h = test_history();
        assert_eq!(h.hash_item("Work/GitHub"), h.hash_item("Work/GitHub"));
    }

    #[test]
    fn hash_item_differs_for_different_items() {
        let h = test_history();
        assert_ne!(h.hash_item("Work/GitHub"), h.hash_item("Work/GitLab"));
    }

    #[test]
    fn hash_item_differs_across_keys() {
        let h1 = test_history();
        let mut h2 = test_history();
        h2.key = Zeroizing::new(vec![7u8; 32]);
        assert_ne!(h1.hash_item("Work/GitHub"), h2.hash_item("Work/GitHub"));
    }

    #[test]
    fn get_score_zero_for_unknown_item() {
        let h = test_history();
        assert_eq!(h.get_score("unknown"), 0);
    }

    #[test]
    fn get_score_disabled_history_is_always_zero() {
        let mut h = test_history();
        h.enabled = false;
        h.records.insert(h.hash_item("x"), (10, 0));
        // hash_item calculado antes de desabilitar não importa: com enabled=false
        // get_score deve retornar 0 independentemente do conteúdo de records.
        assert_eq!(h.get_score("x"), 0);
    }

    #[test]
    fn record_use_increments_count_and_updates_timestamp() {
        let mut h = test_history();
        h.file_path = PathBuf::from("/dev/null");
        h.record_use("Work/GitHub");
        h.record_use("Work/GitHub");
        let hashed = h.hash_item("Work/GitHub");
        let (count, _ts) = h.records.get(&hashed).unwrap();
        assert_eq!(*count, 2);
    }

    #[test]
    fn sort_items_orders_by_score_descending() {
        let mut h = test_history();
        h.file_path = PathBuf::from("/dev/null");
        h.record_use("frequent");
        h.record_use("frequent");
        h.record_use("frequent");
        h.record_use("rare");
        let mut items = vec!["rare".to_string(), "frequent".to_string(), "unused".to_string()];
        h.sort_items(&mut items);
        assert_eq!(items[0], "frequent");
        assert_eq!(items[1], "rare");
        assert_eq!(items[2], "unused");
    }
}
