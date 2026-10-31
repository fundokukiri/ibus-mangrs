//! Backup/restore of settings as one JSON file: config + macros + dictionary.
//! Pure serialization here; `gui.rs` owns the file dialogs and the widget
//! refresh, so round-trip and corrupt-input handling can be tested without GTK.

use serde::{Deserialize, Serialize};

use crate::config::Config;

#[derive(Serialize, Deserialize)]
pub struct Backup {
    pub version: u32,
    pub config: Config,
    pub macros: Vec<(String, String)>,
    pub dictionary: Vec<String>,
}

/// Bump when the layout changes incompatibly; import rejects newer versions
/// rather than half-applying unknown fields.
pub const BACKUP_VERSION: u32 = 1;

pub fn to_json(
    config: Config,
    macros: Vec<(String, String)>,
    dictionary: Vec<String>,
) -> Result<String, String> {
    let backup = Backup {
        version: BACKUP_VERSION,
        config,
        macros,
        dictionary,
    };
    serde_json::to_string_pretty(&backup).map_err(|e| e.to_string())
}

pub fn from_json(text: &str) -> Result<Backup, String> {
    let backup: Backup = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if backup.version > BACKUP_VERSION {
        return Err(format!(
            "backup version {} is newer than this build ({}); upgrade ibus-mangrs first",
            backup.version, BACKUP_VERSION
        ));
    }
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{MethodConfig, ModeConfig};

    fn sample() -> (Config, Vec<(String, String)>, Vec<String>) {
        let mut config = Config::default();
        config.method = MethodConfig::Vni;
        config.mode = ModeConfig::Surrounding;
        config.app_rules = vec![crate::config::AppRule {
            pattern: "kate".into(),
            mode: ModeConfig::Preedit,
        }];
        (
            config,
            vec![("ddq".into(), "đại đội trưởng".into())],
            vec!["asl".into(), "tr".into()],
        )
    }

    #[test]
    fn roundtrip_preserves_all_sections() {
        let (config, macros, dictionary) = sample();
        let text = to_json(config.clone(), macros.clone(), dictionary.clone()).unwrap();
        let back = from_json(&text).unwrap();
        assert_eq!(back.version, BACKUP_VERSION);
        assert_eq!(back.config.method, MethodConfig::Vni);
        assert_eq!(back.config.mode, ModeConfig::Surrounding);
        assert_eq!(back.config.app_rules, config.app_rules);
        assert_eq!(back.macros, macros);
        assert_eq!(back.dictionary, dictionary);
    }

    #[test]
    fn corrupt_json_is_an_error_not_a_panic() {
        assert!(from_json("not json at all").is_err());
        assert!(from_json("{\"version\": 1, }").is_err());
        assert!(from_json("").is_err());
    }

    #[test]
    fn missing_sections_are_rejected() {
        // A truncated file must not silently import a default config.
        let text = to_json(Config::default(), vec![], vec![]).unwrap();
        let stripped = text.split("\"macros\"").next().unwrap();
        let broken = format!("{stripped}}}");
        assert!(from_json(&broken).is_err());
    }

    #[test]
    fn newer_backup_version_is_rejected() {
        let text = to_json(Config::default(), vec![], vec![])
            .unwrap()
            .replacen("\"version\": 1", "\"version\": 99", 1);
        let err = match from_json(&text) {
            Err(e) => e,
            Ok(_) => panic!("newer backup version must be rejected"),
        };
        assert!(err.contains("newer"), "{err}");
    }
}
