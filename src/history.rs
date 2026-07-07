//! Motor de frecency. Os nomes das entradas são armazenados no arquivo de
//! histórico como HMAC-SHA256(chave_local, nome) — a chave aleatória fica em
//! ~/.config/fpass/history.key (0600), o que impede reverter os nomes por
//! rainbow table / dicionário, diferente de um SHA-256 puro.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

pub struct History {
    records: HashMap<String, (u32, u64)>,
    file_path: PathBuf,
    key: Vec<u8>,
    enabled: bool,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Peso decrescente por idade do último uso (mesma curva do fpass 1.x).
pub fn weight_for_age(age_secs: u64) -> u64 {
    if age_secs < 86_400 {
        100
    } else if age_secs < 604_800 {
        50
    } else if age_secs < 2_592_000 {
        20
    } else {
        5
    }
}

fn load_or_create_key(config_dir: &PathBuf) -> Vec<u8> {
    let key_path = config_dir.join("history.key");
    if let Ok(hex) = fs::read_to_string(&key_path) {
        if let Some(bytes) = hex_decode(hex.trim()) {
            if bytes.len() == 32 {
                return bytes;
            }
        }
    }
    // Gera chave nova de 32 bytes
    let mut key = vec![0u8; 32];
    if getrandom::getrandom(&mut key).is_err() {
        // Sem entropia do SO não persistimos chave; frecency ainda funciona
        // dentro da sessão, mas sem histórico entre sessões.
        return key;
    }
    let hex: String = key.iter().map(|b| format!("{:02x}", b)).collect();
    let _ = fs::write(&key_path, hex);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600));
    }
    key
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

impl History {
    pub fn new(enabled: bool, config_dir: &PathBuf) -> Self {
        let file_path = config_dir.join("history");
        let key = load_or_create_key(config_dir);
        let mut records = HashMap::new();

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
        Self {
            records,
            file_path,
            key,
            enabled,
        }
    }

    fn hash_item(&self, item: &str) -> String {
        let mut mac = HmacSha256::new_from_slice(&self.key)
            .expect("HMAC aceita chave de qualquer tamanho");
        mac.update(item.as_bytes());
        mac.finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }

    pub fn record_use(&mut self, item: &str) {
        if !self.enabled {
            return;
        }
        let hashed = self.hash_item(item);
        let entry = self.records.entry(hashed).or_insert((0, 0));
        entry.0 += 1;
        entry.1 = now_secs();
        self.save();
    }

    fn save(&self) {
        if !self.enabled {
            return;
        }
        if let Ok(mut file) = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&self.file_path)
        {
            for (hash, (count, ts)) in &self.records {
                let _ = writeln!(file, "{}|{}|{}", hash, count, ts);
            }
        }
    }

    pub fn get_score(&self, item: &str) -> u64 {
        if !self.enabled {
            return 0;
        }
        let hashed = self.hash_item(item);
        if let Some(&(count, ts)) = self.records.get(&hashed) {
            let age = now_secs().saturating_sub(ts);
            (count as u64) * weight_for_age(age)
        } else {
            0
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weight_curve() {
        assert_eq!(weight_for_age(0), 100);
        assert_eq!(weight_for_age(86_399), 100);
        assert_eq!(weight_for_age(86_400), 50);
        assert_eq!(weight_for_age(604_800), 20);
        assert_eq!(weight_for_age(2_592_000), 5);
        assert_eq!(weight_for_age(u64::MAX), 5);
    }

    #[test]
    fn hex_roundtrip() {
        assert_eq!(hex_decode("00ff10"), Some(vec![0x00, 0xff, 0x10]));
        assert_eq!(hex_decode("0"), None);
        assert_eq!(hex_decode("zz"), None);
    }

    #[test]
    fn frecency_sorting_prefers_recent_use() {
        let tmp = std::env::temp_dir().join(format!("fpass-test-{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let mut h = History::new(true, &tmp);
        h.record_use("b/entry");
        h.record_use("b/entry");
        h.record_use("a/entry");
        let mut items = vec!["a/entry".to_string(), "b/entry".to_string(), "c/entry".to_string()];
        h.sort_items(&mut items);
        assert_eq!(items[0], "b/entry"); // 2 usos > 1 uso > 0 usos
        assert_eq!(items[1], "a/entry");
        assert_eq!(items[2], "c/entry");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn hmac_differs_between_keys() {
        let tmp1 = std::env::temp_dir().join(format!("fpass-k1-{}", std::process::id()));
        let tmp2 = std::env::temp_dir().join(format!("fpass-k2-{}", std::process::id()));
        fs::create_dir_all(&tmp1).unwrap();
        fs::create_dir_all(&tmp2).unwrap();
        let h1 = History::new(true, &tmp1);
        let h2 = History::new(true, &tmp2);
        // Chaves aleatórias distintas => hashes distintos pro mesmo item
        assert_ne!(h1.hash_item("github.com"), h2.hash_item("github.com"));
        let _ = fs::remove_dir_all(&tmp1);
        let _ = fs::remove_dir_all(&tmp2);
    }
}
