//! A single local listener profile — no sign-in needed.
//! (Older saved files with several accounts still load; the first one is used.)

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    pub username: String,
    pub display_name: String,
    #[serde(default)]
    pub created: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct AccountStore {
    #[serde(default)]
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub remembered: Option<String>,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl AccountStore {
    pub fn load(dir: &Path) -> Self {
        std::fs::read_to_string(dir.join("accounts.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) {
        let _ = std::fs::create_dir_all(dir);
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(dir.join("accounts.json"), s);
        }
    }

    /// The profile to open straight away: the last used one, else the first,
    /// else a fresh one is created.
    pub fn profile(&mut self) -> Account {
        if let Some(u) = &self.remembered {
            if let Some(a) = self.accounts.iter().find(|a| &a.username == u) {
                return a.clone();
            }
        }
        if let Some(a) = self.accounts.first() {
            self.remembered = Some(a.username.clone());
            return a.clone();
        }
        let a = Account { username: "me".into(), display_name: "Listener".into(), created: now_secs() };
        self.accounts.push(a.clone());
        self.remembered = Some(a.username.clone());
        a
    }

    pub fn rename(&mut self, username: &str, display: &str) {
        if let Some(a) = self.accounts.iter_mut().find(|a| a.username == username) {
            a.display_name = display.trim().to_string();
        }
    }
}

pub fn member_since(secs: u64) -> String {
    // Civil-from-days (Howard Hinnant), good enough for a "listening since" line.
    let days = (secs / 86_400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let names = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    format!("{} {}", names[(m - 1) as usize], y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_a_profile_without_signing_in() {
        let mut s = AccountStore::default();
        let a = s.profile();
        assert_eq!(a.display_name, "Listener");
        assert_eq!(s.profile().username, a.username, "same profile next time");
        s.rename("me", "Sam");
        assert_eq!(s.profile().display_name, "Sam");
        // old saved file with password fields still loads
        let old = r#"{"accounts":[{"username":"x","display_name":"X","salt":"a","hash":"b","created":5}],"remembered":null}"#;
        let mut s: AccountStore = serde_json::from_str(old).unwrap();
        assert_eq!(s.profile().username, "x");
    }
}
