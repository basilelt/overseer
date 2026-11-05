use serde::{Deserialize, Serialize};
use std::path::PathBuf;

fn ssh_port() -> u16 {
    22
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub host: String,
    #[serde(default = "ssh_port")]
    pub port: u16,
    #[serde(default)]
    pub user: String,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Auth {
    #[default]
    Key,
    Password,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SavedHost {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub label: String,
    pub host: String,
    #[serde(default = "ssh_port")]
    pub port: u16,
    #[serde(default)]
    pub user: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jump: Option<Endpoint>,
    #[serde(default)]
    pub auth: Auth,
    #[serde(default)]
    pub remember_password: bool,
}

impl SavedHost {
    pub fn endpoint(&self) -> Endpoint {
        Endpoint {
            host: self.host.clone(),
            port: self.port,
            user: self.user.clone(),
        }
    }

    pub fn describe(&self) -> String {
        let main = describe(&self.endpoint());
        match &self.jump {
            Some(j) => format!("{main} via ProxyJump {}", describe(j)),
            None => main,
        }
    }
}

pub fn describe(e: &Endpoint) -> String {
    format!("{}@{}:{}", e.user, e.host, e.port)
}

fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && !s
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '@')
}

pub fn validate(h: &SavedHost) -> Result<(), String> {
    for (what, e) in
        std::iter::once(("host", h.endpoint())).chain(h.jump.clone().map(|j| ("jump host", j)))
    {
        if !valid_name(e.host.trim()) {
            return Err(format!("the {what} name is empty or has spaces"));
        }
        if !valid_name(e.user.trim()) {
            return Err(format!("the {what} user is empty or has spaces"));
        }
        if e.port == 0 {
            return Err(format!("the {what} port must be 1 to 65535"));
        }
    }
    Ok(())
}

pub struct Hosts {
    path: PathBuf,
}

impl Hosts {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn list(&self) -> Vec<SavedHost> {
        std::fs::read(&self.path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn get(&self, id: &str) -> Option<SavedHost> {
        self.list().into_iter().find(|h| h.id == id)
    }

    pub fn save(&self, mut h: SavedHost) -> Result<Vec<SavedHost>, String> {
        h.host = h.host.trim().to_string();
        h.user = h.user.trim().to_string();
        h.label = h.label.trim().to_string();
        if let Some(j) = &mut h.jump {
            j.host = j.host.trim().to_string();
            j.user = j.user.trim().to_string();
            if j.user.is_empty() {
                j.user.clone_from(&h.user);
            }
        }
        h.jump = h.jump.filter(|j| !j.host.is_empty());
        validate(&h)?;
        if h.label.is_empty() {
            h.label.clone_from(&h.host);
        }
        let mut all = self.list();
        if h.id.is_empty() {
            h.id = new_id(&all);
        }
        match all.iter_mut().find(|x| x.id == h.id) {
            Some(x) => *x = h,
            None => all.push(h),
        }
        self.write(&all)?;
        Ok(all)
    }

    pub fn delete(&self, id: &str) -> Result<Vec<SavedHost>, String> {
        let mut all = self.list();
        all.retain(|h| h.id != id);
        self.write(&all)?;
        Ok(all)
    }

    fn write(&self, all: &[SavedHost]) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let body = serde_json::to_vec_pretty(all).map_err(|e| e.to_string())?;
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, body)
            .and_then(|_| std::fs::rename(&tmp, &self.path))
            .map_err(|e| format!("{}: {e}", self.path.display()))
    }
}

fn new_id(all: &[SavedHost]) -> String {
    (1..)
        .map(|n| format!("h{n}"))
        .find(|id| !all.iter().any(|h| &h.id == id))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_updates_and_deletes() {
        let dir = std::env::temp_dir().join(format!("sb-hosts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let hosts = Hosts::new(dir.join("hosts.json"));
        assert!(hosts.list().is_empty());
        let h = SavedHost {
            host: " box.lan ".into(),
            user: "alice".into(),
            port: 22,
            ..Default::default()
        };
        let all = hosts.save(h).unwrap();
        assert_eq!(
            (
                all[0].id.as_str(),
                all[0].label.as_str(),
                all[0].host.as_str()
            ),
            ("h1", "box.lan", "box.lan")
        );
        let jump = Some(Endpoint {
            host: "bastion".into(),
            port: 22,
            user: String::new(),
        });
        let all = hosts
            .save(SavedHost {
                host: "10.0.0.5".into(),
                user: "root".into(),
                port: 2222,
                jump,
                auth: Auth::Password,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[1].jump.as_ref().unwrap().user, "root");
        assert_eq!(
            all[1].describe(),
            "root@10.0.0.5:2222 via ProxyJump root@bastion:22"
        );
        let mut first = all[0].clone();
        first.label = "Workstation".into();
        hosts.save(first).unwrap();
        assert_eq!(hosts.get("h1").unwrap().label, "Workstation");
        assert_eq!(hosts.delete("h1").unwrap().len(), 1);
        assert_eq!(
            hosts
                .save(SavedHost {
                    host: "x".into(),
                    user: "u".into(),
                    port: 22,
                    ..Default::default()
                })
                .unwrap()[1]
                .id,
            "h1"
        );
        assert!(hosts
            .save(SavedHost {
                host: "two words".into(),
                user: "u".into(),
                port: 22,
                ..Default::default()
            })
            .is_err());
        assert!(hosts
            .save(SavedHost {
                host: "box".into(),
                user: String::new(),
                port: 22,
                ..Default::default()
            })
            .is_err());
        let json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("hosts.json")).unwrap()).unwrap();
        assert_eq!(json[0]["auth"], "password");
        assert_eq!(json[0]["rememberPassword"], false);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
