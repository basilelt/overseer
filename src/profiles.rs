use crate::agents::Agent;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub type Res<T> = Result<T, (u16, String)>;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Sub {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub model: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub effort: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct AgentCfg {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub model: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub effort: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub subagent_model: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub subagent_effort: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub subagent_force: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub small_model: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub subagents: BTreeMap<String, Sub>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    #[serde(skip_deserializing, skip_serializing_if = "std::ops::Not::not")]
    pub builtin: bool,
    pub agents: BTreeMap<String, AgentCfg>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Store {
    pub default: String,
    pub profiles: Vec<Profile>,
    pub scopes: BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Chip {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub model: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub effort: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Applied {
    pub model: String,
    pub effort: String,
    pub extra: Vec<String>,
    pub env: Vec<(String, String)>,
    pub chip: Option<Chip>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Via {
    Flag,
    Env,
    File,
}

impl Via {
    fn as_str(self) -> &'static str {
        match self {
            Via::Flag => "flag",
            Via::Env => "env",
            Via::File => "file",
        }
    }
}

pub struct Support {
    pub model: Option<Via>,
    pub effort: Option<Via>,
    pub subagent_model: Option<Via>,
    pub subagent_effort: Option<Via>,
    pub small_model: Option<Via>,
    pub subagents: Option<Via>,
    pub subagent_efforts: bool,
    pub force: bool,
}

const NO: Support = Support {
    model: None,
    effort: None,
    subagent_model: None,
    subagent_effort: None,
    small_model: None,
    subagents: None,
    subagent_efforts: false,
    force: false,
};

pub fn support(a: &Agent) -> Support {
    let base = Support {
        model: a.model_flag.map(|_| Via::Flag),
        effort: a.effort.map(|_| Via::Flag),
        ..NO
    };
    match a.id {
        "claude" => Support {
            subagent_model: Some(Via::Env),
            small_model: Some(Via::Env),
            subagents: Some(Via::File),
            subagent_efforts: true,
            force: true,
            ..base
        },
        "codex" => Support {
            subagent_model: Some(Via::Flag),
            subagent_effort: Some(Via::Flag),
            ..base
        },
        "gemini" | "agy" => Support {
            subagents: Some(Via::File),
            ..base
        },
        "opencode" => Support {
            small_model: Some(Via::Env),
            subagents: Some(Via::Env),
            ..base
        },
        "shell" => NO,
        _ => base,
    }
}

fn support_json(s: &Support) -> Value {
    let mut v = json!({});
    for (k, via) in [
        ("model", s.model),
        ("effort", s.effort),
        ("subagentModel", s.subagent_model),
        ("subagentEffort", s.subagent_effort),
        ("smallModel", s.small_model),
        ("subagents", s.subagents),
    ] {
        if let Some(via) = via {
            v[k] = json!(via.as_str());
        }
    }
    if s.subagent_efforts {
        v["subagentEfforts"] = json!(true);
    }
    if s.force {
        v["subagentForce"] = json!(true);
    }
    v
}

struct Row<'a>(&'a str, &'a str, &'a str, &'a str, &'a str, &'a str, bool);

fn cfg(rows: &[Row]) -> BTreeMap<String, AgentCfg> {
    rows.iter()
        .map(|Row(id, model, effort, sub, sub_effort, small, force)| {
            (
                id.to_string(),
                AgentCfg {
                    model: model.to_string(),
                    effort: effort.to_string(),
                    subagent_model: sub.to_string(),
                    subagent_effort: sub_effort.to_string(),
                    subagent_force: *force,
                    small_model: small.to_string(),
                    ..Default::default()
                },
            )
        })
        .collect()
}

pub fn builtins() -> Vec<Profile> {
    let p = |id: &str, name: &str, agents| Profile {
        id: id.into(),
        name: name.into(),
        builtin: true,
        agents,
    };
    vec![
        p(
            "economy",
            "Economy",
            cfg(&[
                Row("claude", "sonnet", "low", "haiku", "", "", true),
                Row(
                    "codex",
                    "gpt-5.4-mini",
                    "low",
                    "gpt-5.4-mini",
                    "low",
                    "",
                    false,
                ),
                Row("copilot", "gpt-5-mini", "low", "", "", "", false),
                Row("gemini", "gemini-2.5-flash", "", "", "", "", false),
                Row("agy", "", "low", "", "", "", false),
                Row("amp", "", "low", "", "", "", false),
            ]),
        ),
        p(
            "balanced",
            "Balanced",
            cfg(&[
                Row("claude", "sonnet", "medium", "sonnet", "", "", false),
                Row(
                    "codex",
                    "gpt-5.4",
                    "medium",
                    "gpt-5.4-mini",
                    "medium",
                    "",
                    false,
                ),
                Row("copilot", "", "medium", "", "", "", false),
                Row("agy", "", "medium", "", "", "", false),
                Row("amp", "", "medium", "", "", "", false),
            ]),
        ),
        p(
            "max",
            "Max",
            cfg(&[
                Row("claude", "opus", "max", "", "", "", false),
                Row("codex", "gpt-5.5", "max", "", "", "", false),
                Row("copilot", "", "max", "", "", "", false),
                Row("agy", "", "max", "", "", "", false),
                Row("amp", "", "max", "", "", "", false),
            ]),
        ),
    ]
}

pub fn path(state_dir: &Path) -> PathBuf {
    state_dir.join("profiles.json")
}

pub fn load(state_dir: &Path) -> Store {
    std::fs::read(path(state_dir))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn all(s: &Store) -> Vec<Profile> {
    let mut v = builtins();
    v.extend(s.profiles.iter().cloned());
    v
}

pub fn find(s: &Store, id: &str) -> Option<Profile> {
    all(s).into_iter().find(|p| p.id == id)
}

fn under(path: &str, root: &str) -> bool {
    let root = root.trim_end_matches('/');
    !root.is_empty()
        && (path == root
            || path
                .strip_prefix(root)
                .is_some_and(|rest| rest.starts_with('/')))
}

pub fn scoped<'a>(s: &'a Store, cwd: &str) -> Option<&'a str> {
    s.scopes
        .iter()
        .filter(|(root, _)| under(cwd, root))
        .max_by_key(|(root, _)| root.len())
        .map(|(_, id)| id.as_str())
}

pub fn resolve(s: &Store, explicit: Option<&str>, cwds: &[&str]) -> Res<Option<Profile>> {
    let id = match explicit {
        Some(id) => id,
        None => cwds
            .iter()
            .find_map(|c| scoped(s, c))
            .unwrap_or(s.default.as_str()),
    };
    if id.is_empty() || id == "none" {
        return Ok(None);
    }
    match find(s, id) {
        Some(p) => Ok(Some(p)),
        None if explicit.is_some() => Err((400, format!("unknown profile {id}"))),
        None => Ok(None),
    }
}

fn opencode_config(c: &AgentCfg) -> Option<String> {
    let mut v = json!({});
    if !c.small_model.is_empty() {
        v["small_model"] = json!(c.small_model);
    }
    for (name, sub) in &c.subagents {
        if !sub.model.is_empty() {
            v["agent"][name] = json!({ "model": sub.model });
        }
    }
    (v != json!({})).then(|| v.to_string())
}

pub fn apply(a: &Agent, p: Option<&Profile>, model: &str, effort: &str) -> Applied {
    let empty = AgentCfg::default();
    let c = p.and_then(|p| p.agents.get(a.id)).unwrap_or(&empty);
    let s = support(a);
    let pick = |explicit: &str, from: &str, ok: bool| {
        if !explicit.is_empty() {
            explicit.to_string()
        } else if ok {
            from.to_string()
        } else {
            String::new()
        }
    };
    let model = pick(model, &c.model, s.model.is_some());
    let effort = pick(effort, &c.effort, s.effort.is_some());
    let mut env = vec![];
    let mut extra = vec![];
    match a.id {
        "claude" => {
            if !c.subagent_model.is_empty() {
                env.push((
                    "CLAUDE_CODE_SUBAGENT_MODEL".into(),
                    c.subagent_model.clone(),
                ));
                if c.subagent_force {
                    env.push(("CLAUDE_CODE_SUBAGENT_MODEL_FORCE".into(), "1".into()));
                }
            }
            if !c.small_model.is_empty() {
                env.push((
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL".into(),
                    c.small_model.clone(),
                ));
            }
        }
        "codex" => {
            if !c.subagent_model.is_empty() {
                extra.extend([
                    "-c".to_string(),
                    format!("agents.default_subagent_model=\"{}\"", c.subagent_model),
                ]);
            }
            if !c.subagent_effort.is_empty() {
                let e = if c.subagent_effort == "max" {
                    "xhigh"
                } else {
                    c.subagent_effort.as_str()
                };
                extra.extend([
                    "-c".to_string(),
                    format!("agents.default_subagent_reasoning_effort=\"{e}\""),
                ]);
            }
        }
        "opencode" => {
            if let Some(cfg) = opencode_config(c) {
                env.push(("OPENCODE_CONFIG_CONTENT".into(), cfg));
            }
        }
        _ => {}
    }
    let chip = (p.is_some() || !model.is_empty() || !effort.is_empty()).then(|| Chip {
        id: p.map(|p| p.id.clone()).unwrap_or_default(),
        name: p.map(|p| p.name.clone()).unwrap_or_default(),
        model: model.clone(),
        effort: effort.clone(),
    });
    Applied {
        model,
        effort,
        extra,
        env,
        chip,
    }
}

fn valid_value(v: &str) -> bool {
    v.len() <= 128
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || "._:/-[]@".contains(c))
}

fn valid_name(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 64
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
}

fn check_effort(e: &str, what: &str, levels: &[&str]) -> Res<()> {
    if e.is_empty() {
        return Ok(());
    }
    if levels.is_empty() {
        return Err((400, format!("{what}: effort is not supported")));
    }
    if levels.contains(&e) {
        Ok(())
    } else {
        Err((
            400,
            format!("{what}: effort must be one of {}", levels.join(", ")),
        ))
    }
}

fn check_model(m: &str, what: &str) -> Res<()> {
    if valid_value(m) {
        Ok(())
    } else {
        Err((400, format!("{what}: invalid model {m:?}")))
    }
}

pub fn validate(s: &Store) -> Res<()> {
    let builtin: Vec<String> = builtins().into_iter().map(|p| p.id).collect();
    let mut seen: Vec<&str> = vec![];
    for p in &s.profiles {
        if !valid_name(&p.id) || p.id == "none" || builtin.contains(&p.id) {
            return Err((400, format!("invalid or reserved profile id {:?}", p.id)));
        }
        if seen.contains(&p.id.as_str()) {
            return Err((400, format!("duplicate profile id {}", p.id)));
        }
        seen.push(&p.id);
        let name = p.name.trim();
        if name.is_empty() || name.len() > 40 || name.chars().any(char::is_control) {
            return Err((
                400,
                format!("profile {}: name must be 1-40 characters", p.id),
            ));
        }
        for (agent, c) in &p.agents {
            let Some(a) = crate::agents::get(agent).filter(|a| a.id != "shell") else {
                return Err((400, format!("profile {}: unknown agent {agent}", p.id)));
            };
            let what = format!("profile {} / {}", p.id, a.name);
            let sp = support(a);
            for (val, ok, field) in [
                (&c.model, sp.model.is_some(), "model"),
                (&c.effort, sp.effort.is_some(), "effort"),
                (
                    &c.subagent_model,
                    sp.subagent_model.is_some(),
                    "subagentModel",
                ),
                (
                    &c.subagent_effort,
                    sp.subagent_effort.is_some(),
                    "subagentEffort",
                ),
                (&c.small_model, sp.small_model.is_some(), "smallModel"),
            ] {
                if !val.is_empty() && !ok {
                    return Err((400, format!("{what}: {field} is not supported")));
                }
            }
            if c.subagent_force && !sp.force {
                return Err((
                    400,
                    format!("{what}: forcing the subagent model is not supported"),
                ));
            }
            if !c.subagents.is_empty() && sp.subagents.is_none() {
                return Err((400, format!("{what}: subagent overrides are not supported")));
            }
            check_model(&c.model, &what)?;
            check_model(&c.subagent_model, &what)?;
            check_model(&c.small_model, &what)?;
            check_effort(&c.effort, &what, a.efforts())?;
            check_effort(&c.subagent_effort, &what, a.efforts())?;
            for (name, sub) in &c.subagents {
                if !valid_name(name) {
                    return Err((400, format!("{what}: invalid subagent name {name:?}")));
                }
                check_model(&sub.model, &what)?;
                if agent == "agy"
                    && !sub.model.is_empty()
                    && !["inherit", "flash", "pro"].contains(&sub.model.as_str())
                {
                    return Err((
                        400,
                        format!(
                            "{what}: Antigravity subagent model must be inherit, flash, or pro"
                        ),
                    ));
                }
                check_effort(&sub.effort, &what, a.efforts())?;
                if !sub.effort.is_empty() && !sp.subagent_efforts {
                    return Err((400, format!("{what}: subagent effort is not supported")));
                }
            }
        }
    }
    let ids: Vec<String> = all(s).into_iter().map(|p| p.id).collect();
    let known = |id: &str| id.is_empty() || id == "none" || ids.iter().any(|x| x == id);
    if !known(&s.default) {
        return Err((400, format!("unknown default profile {}", s.default)));
    }
    for (root, id) in &s.scopes {
        if !root.starts_with('/') && !root.contains(":\\") {
            return Err((400, format!("scope {root:?} must be an absolute path")));
        }
        if id.is_empty() || !known(id) {
            return Err((400, format!("scope {root}: unknown profile {id}")));
        }
    }
    Ok(())
}

pub fn save(state_dir: &Path, s: &Store) -> Res<()> {
    validate(s)?;
    let mut s = s.clone();
    for p in &mut s.profiles {
        p.name = p.name.trim().to_string();
        p.agents.retain(|_, c| *c != AgentCfg::default());
    }
    let text = serde_json::to_string_pretty(&s).map_err(|e| (500, e.to_string()))?;
    crate::install::atomic_write(&path(state_dir), &text).map_err(|e| (500, e))
}

pub fn to_json(s: &Store) -> Value {
    let agents: serde_json::Map<String, Value> = crate::agents::AGENTS
        .iter()
        .filter(|a| a.id != "shell")
        .map(|a| (a.id.to_string(), support_json(&support(a))))
        .collect();
    json!({
        "default": s.default,
        "profiles": all(s),
        "scopes": s.scopes,
        "support": agents,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct SubagentDef {
    pub agent: &'static str,
    pub name: String,
    pub scope: &'static str,
    pub path: PathBuf,
    pub model: String,
    pub effort: String,
}

impl SubagentDef {
    fn to_json(&self) -> Value {
        json!({
            "agent": self.agent, "name": self.name, "scope": self.scope,
            "path": self.path.to_string_lossy(), "model": self.model, "effort": self.effort,
        })
    }
}

fn frontmatter(text: &str) -> Option<(usize, usize)> {
    let start = if text.starts_with("---\n") {
        4
    } else if text.starts_with("---\r\n") {
        5
    } else {
        return None;
    };
    let mut pos = start;
    for line in text[start..].split_inclusive('\n') {
        if line.trim_end() == "---" {
            return Some((start, pos));
        }
        pos += line.len();
    }
    None
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    let v = v.split(" #").next().unwrap_or(v).trim();
    v.trim_matches(|c| c == '"' || c == '\'').to_string()
}

pub fn front_get(text: &str, key: &str) -> Option<String> {
    let (a, b) = frontmatter(text)?;
    text[a..b].lines().find_map(|l| {
        l.strip_prefix(key)
            .and_then(|r| r.strip_prefix(':'))
            .map(unquote)
    })
}

pub fn front_set(text: &str, key: &str, value: &str) -> Option<String> {
    let (a, b) = frontmatter(text)?;
    let nl = if text[..a].ends_with("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut out = String::with_capacity(text.len() + 32);
    out.push_str(&text[..a]);
    let mut done = false;
    for line in text[a..b].split_inclusive('\n') {
        let is_key = line.strip_prefix(key).is_some_and(|r| r.starts_with(':'));
        if !is_key {
            out.push_str(line);
        } else if !done && !value.is_empty() {
            out.push_str(&format!("{key}: {value}{nl}"));
            done = true;
        } else {
            done = true;
        }
    }
    if !done && !value.is_empty() {
        out.push_str(&format!("{key}: {value}{nl}"));
    }
    out.push_str(&text[b..]);
    Some(out)
}

fn md_defs(dir: &Path, agent: &'static str, scope: &'static str) -> Vec<SubagentDef> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let paths: Vec<PathBuf> = rd
        .flatten()
        .flat_map(|e| {
            let path = e.path();
            if path.extension().is_some_and(|x| x == "md") {
                vec![path]
            } else if path.is_dir() {
                vec![path.join("agent.md")]
            } else {
                vec![]
            }
        })
        .collect();
    let mut v: Vec<SubagentDef> = paths
        .into_iter()
        .filter_map(|p| {
            if p.extension().is_none_or(|x| x != "md") {
                return None;
            }
            let meta = std::fs::metadata(&p).ok()?;
            if !meta.is_file() || meta.len() > 512 * 1024 {
                return None;
            }
            let text = std::fs::read_to_string(&p).ok()?;
            let stem = p.file_stem()?.to_string_lossy().into_owned();
            let name = front_get(&text, "name")
                .filter(|n| !n.is_empty())
                .unwrap_or(stem);
            Some(SubagentDef {
                agent,
                name,
                scope,
                model: front_get(&text, "model").unwrap_or_default(),
                effort: front_get(&text, "effort").unwrap_or_default(),
                path: p,
            })
        })
        .collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

fn opencode_json_defs(file: &Path, scope: &'static str) -> Vec<SubagentDef> {
    let Some(v) = std::fs::read(file)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
    else {
        return vec![];
    };
    let Some(map) = v["agent"].as_object() else {
        return vec![];
    };
    map.iter()
        .map(|(name, a)| SubagentDef {
            agent: "opencode",
            name: name.clone(),
            scope,
            path: file.to_path_buf(),
            model: a["model"].as_str().unwrap_or_default().to_string(),
            effort: String::new(),
        })
        .collect()
}

pub struct Dirs {
    pub claude: PathBuf,
    pub gemini: PathBuf,
    pub opencode: PathBuf,
}

impl Dirs {
    pub fn from_env(claude_dir: &Path, gemini_dir: &Path) -> Self {
        Dirs {
            claude: claude_dir.to_path_buf(),
            gemini: gemini_dir.to_path_buf(),
            opencode: std::env::var_os("XDG_CONFIG_HOME")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| crate::home().join(".config"))
                .join("opencode"),
        }
    }
}

fn merge(mut project: Vec<SubagentDef>, user: Vec<SubagentDef>) -> Vec<SubagentDef> {
    for u in user {
        if !project.iter().any(|p| p.name == u.name) {
            project.push(u);
        }
    }
    project.sort_by(|a, b| a.name.cmp(&b.name).then(a.scope.cmp(b.scope)));
    project
}

pub fn subagents(d: &Dirs, project: Option<&Path>) -> Vec<SubagentDef> {
    let mut claude_project = vec![];
    let mut oc_project = vec![];
    let mut gemini_project = vec![];
    let mut agy_project = vec![];
    if let Some(p) = project {
        claude_project = md_defs(&p.join(".claude/agents"), "claude", "project");
        gemini_project = md_defs(&p.join(".gemini/agents"), "gemini", "project");
        agy_project = md_defs(&p.join(".agents/agents"), "agy", "project");
        for sub in ["agent", "agents"] {
            oc_project.extend(md_defs(
                &p.join(".opencode").join(sub),
                "opencode",
                "project",
            ));
        }
        oc_project.extend(opencode_json_defs(&p.join("opencode.json"), "project"));
    }
    let claude_user = md_defs(&d.claude.join("agents"), "claude", "user");
    let gemini_user = md_defs(&d.gemini.join("agents"), "gemini", "user");
    let agy_user = md_defs(&d.gemini.join("config/agents"), "agy", "user");
    let mut oc_user = vec![];
    for sub in ["agent", "agents"] {
        oc_user.extend(md_defs(&d.opencode.join(sub), "opencode", "user"));
    }
    oc_user.extend(opencode_json_defs(
        &d.opencode.join("opencode.json"),
        "user",
    ));
    let mut v = merge(claude_project, claude_user);
    v.extend(merge(gemini_project, gemini_user));
    v.extend(merge(agy_project, agy_user));
    v.extend(merge(oc_project, oc_user));
    v
}

#[derive(Clone, Debug, PartialEq)]
pub struct FileEdit {
    pub path: PathBuf,
    pub name: String,
    pub scope: &'static str,
    pub changes: Vec<(String, String, String)>,
    pub after: String,
}

impl FileEdit {
    fn to_json(&self) -> Value {
        json!({
            "path": self.path.to_string_lossy(), "name": self.name, "scope": self.scope,
            "changes": self.changes.iter().map(|(k, from, to)| json!({"key": k, "from": from, "to": to})).collect::<Vec<_>>(),
        })
    }
}

pub fn plan_files(defs: &[SubagentDef], c: &AgentCfg) -> Vec<FileEdit> {
    let mut out = vec![];
    for d in defs
        .iter()
        .filter(|d| matches!(d.agent, "claude" | "gemini" | "agy"))
    {
        let Some(sub) = c.subagents.get(&d.name) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(&d.path) else {
            continue;
        };
        let mut after = text.clone();
        let mut changes = vec![];
        let mut fields = vec![("model", &sub.model, &d.model)];
        if d.agent == "claude" {
            fields.push(("effort", &sub.effort, &d.effort));
        }
        for (key, want, have) in fields {
            if want.is_empty() || want == have {
                continue;
            }
            if let Some(next) = front_set(&after, key, want) {
                after = next;
                changes.push((key.to_string(), have.clone(), want.clone()));
            }
        }
        if !changes.is_empty() {
            out.push(FileEdit {
                path: d.path.clone(),
                name: d.name.clone(),
                scope: d.scope,
                changes,
                after,
            });
        }
    }
    out
}

pub fn write_file(e: &FileEdit, stamp: &str) -> Result<PathBuf, String> {
    let b = PathBuf::from(format!("{}.bak-{stamp}", e.path.display()));
    std::fs::copy(&e.path, &b).map_err(|err| format!("backup {}: {err}", b.display()))?;
    crate::install::atomic_write(&e.path, &e.after)?;
    Ok(b)
}

fn project_of(cwd: Option<&str>) -> Option<PathBuf> {
    let cwd = cwd.filter(|c| !c.is_empty())?;
    let root = crate::tasks::main_root(&mut crate::git::Git::default(), cwd)
        .map(|(r, _)| r)
        .unwrap_or_else(|| cwd.to_string());
    Some(PathBuf::from(root))
}

pub fn for_launch(
    state_dir: &Path,
    a: &Agent,
    explicit: Option<&str>,
    cwds: &[&str],
    model: &str,
    effort: &str,
) -> Res<Applied> {
    let s = load(state_dir);
    let p = resolve(&s, explicit, cwds)?;
    Ok(apply(a, p.as_ref(), model, effort))
}

pub fn for_task(state_dir: &Path, a: &Agent, t: &crate::tasks::Task) -> Applied {
    let s = load(state_dir);
    let explicit = t
        .profile
        .as_deref()
        .filter(|id| id.is_empty() || *id == "none" || find(&s, id).is_some());
    let p = resolve(&s, explicit, &[t.project.as_str()]).ok().flatten();
    apply(a, p.as_ref(), &t.model, &t.effort)
}

pub fn for_resume(state_dir: &Path, a: &Agent, cwd: &str) -> Applied {
    let root = project_of(Some(cwd)).map(|p| p.to_string_lossy().into_owned());
    let mut cwds = vec![cwd];
    if let Some(r) = root.as_deref() {
        cwds.push(r);
    }
    let s = load(state_dir);
    match resolve(&s, None, &cwds).ok().flatten() {
        Some(p) => {
            let mut x = apply(a, Some(&p), "", "");
            if let Some(c) = x.chip.as_mut() {
                c.model.clear();
                c.effort.clear();
            }
            x
        }
        None => Applied::default(),
    }
}

pub fn routes(
    ctx: &crate::Ctx,
    r: &crate::daemon::Request,
    segs: &[&str],
    body: &Value,
) -> Option<(u16, Value)> {
    let q = |k: &str| crate::daemon::q(r, k).map(crate::board::decode);
    let res = |r: Res<Value>| match r {
        Ok(v) => (200, v),
        Err((c, e)) => (c, json!({ "error": e })),
    };
    Some(match (r.method.as_str(), segs) {
        ("GET", ["v1", "profiles"]) => (200, to_json(&load(&ctx.state_dir))),
        ("PUT", ["v1", "profiles"]) => {
            let mut s = load(&ctx.state_dir);
            let parsed = (|| -> Res<()> {
                if !body.is_object() {
                    return Err((400, "JSON object expected".into()));
                }
                if let Some(v) = body.get("default") {
                    s.default = v
                        .as_str()
                        .ok_or((400, "default must be a string".to_string()))?
                        .into();
                }
                if let Some(v) = body.get("profiles") {
                    let user: Vec<Profile> = serde_json::from_value(v.clone())
                        .map_err(|e| (400, format!("profiles: {e}")))?;
                    s.profiles = user.into_iter().filter(|p| !p.builtin).collect();
                }
                if let Some(v) = body.get("scopes") {
                    s.scopes = serde_json::from_value(v.clone())
                        .map_err(|e| (400, format!("scopes: {e}")))?;
                }
                Ok(())
            })();
            match parsed.and_then(|_| save(&ctx.state_dir, &s)) {
                Ok(()) => {
                    crate::board::publish("profiles.changed", json!({}));
                    (200, to_json(&s))
                }
                Err((c, e)) => (c, json!({ "error": e })),
            }
        }
        ("GET", ["v1", "profiles", "subagents"]) => {
            let project = project_of(q("cwd").as_deref());
            let defs = subagents(
                &Dirs::from_env(&ctx.claude_dir, &ctx.gemini_dir),
                project.as_deref(),
            );
            (
                200,
                json!({ "subagents": defs.iter().map(SubagentDef::to_json).collect::<Vec<_>>() }),
            )
        }
        ("POST", ["v1", "profiles", "apply-files"]) => res(apply_files(ctx, body)),
        _ => return None,
    })
}

fn apply_files(ctx: &crate::Ctx, body: &Value) -> Res<Value> {
    let s = load(&ctx.state_dir);
    let id = body["profile"]
        .as_str()
        .ok_or((400, "profile required".to_string()))?;
    let p = find(&s, id).ok_or_else(|| (400, format!("unknown profile {id}")))?;
    let project = project_of(body["cwd"].as_str());
    let defs = subagents(
        &Dirs::from_env(&ctx.claude_dir, &ctx.gemini_dir),
        project.as_deref(),
    );
    let plan: Vec<FileEdit> = p
        .agents
        .iter()
        .flat_map(|(agent, c)| {
            let mine: Vec<_> = defs.iter().filter(|d| d.agent == agent).cloned().collect();
            plan_files(&mine, c)
        })
        .collect();
    let dry = body["dryRun"].as_bool() != Some(false);
    if dry {
        return Ok(json!({
            "dryRun": true,
            "files": plan.iter().map(FileEdit::to_json).collect::<Vec<_>>(),
        }));
    }
    let wanted: Vec<&str> = body["files"]
        .as_array()
        .ok_or((400, "files must list the confirmed paths".to_string()))?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let stamp = crate::install::stamp(crate::procs::now_ms());
    let results: Vec<Value> = plan
        .iter()
        .filter(|e| wanted.contains(&e.path.to_string_lossy().as_ref()))
        .map(|e| {
            let mut v = e.to_json();
            match write_file(e, &stamp) {
                Ok(b) => {
                    v["written"] = json!(true);
                    v["backup"] = json!(b.to_string_lossy());
                }
                Err(err) => {
                    v["written"] = json!(false);
                    v["error"] = json!(err);
                }
            }
            v
        })
        .collect();
    Ok(json!({ "dryRun": false, "files": results }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::{get, Launch};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sb-profiles-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn store() -> Store {
        let mut agents = BTreeMap::new();
        agents.insert(
            "claude".to_string(),
            AgentCfg {
                model: "haiku".into(),
                ..Default::default()
            },
        );
        Store {
            default: "balanced".into(),
            profiles: vec![Profile {
                id: "mine".into(),
                name: "Mine".into(),
                builtin: false,
                agents,
            }],
            scopes: BTreeMap::from([
                ("/w/app".to_string(), "economy".to_string()),
                ("/w/app/sub".to_string(), "mine".to_string()),
                ("/w/gone".to_string(), "deleted".to_string()),
            ]),
        }
    }

    fn id(r: Res<Option<Profile>>) -> Option<String> {
        r.unwrap().map(|p| p.id)
    }

    #[test]
    fn precedence_task_over_workspace_over_global() {
        let s = store();
        assert_eq!(
            id(resolve(&s, None, &["/elsewhere"])).as_deref(),
            Some("balanced")
        );
        assert_eq!(
            id(resolve(&s, None, &["/w/app"])).as_deref(),
            Some("economy")
        );
        assert_eq!(
            id(resolve(&s, None, &["/w/app/x/y"])).as_deref(),
            Some("economy")
        );
        assert_eq!(
            id(resolve(&s, None, &["/w/app/sub/z"])).as_deref(),
            Some("mine")
        );
        assert_eq!(
            id(resolve(&s, None, &["/w/application"])).as_deref(),
            Some("balanced")
        );
        assert_eq!(
            id(resolve(&s, None, &["/tmp/wt", "/w/app"])).as_deref(),
            Some("economy")
        );
        assert_eq!(
            id(resolve(&s, Some("max"), &["/w/app"])).as_deref(),
            Some("max")
        );
        assert_eq!(id(resolve(&s, Some("none"), &["/w/app"])), None);
        assert_eq!(id(resolve(&s, Some(""), &["/w/app"])), None);
        assert_eq!(id(resolve(&s, None, &["/w/gone"])), None);
        assert!(resolve(&s, Some("nope"), &["/"]).is_err());
        assert_eq!(id(resolve(&Store::default(), None, &["/w/app"])), None);
    }

    fn applied(agent: &str, profile: &str, model: &str, effort: &str) -> Applied {
        let s = store();
        let p = resolve(&s, Some(profile), &[]).unwrap();
        apply(get(agent).unwrap(), p.as_ref(), model, effort)
    }

    fn env(a: &Applied) -> Vec<(&str, &str)> {
        a.env
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect()
    }

    #[test]
    fn claude_economy_flags_and_env() {
        let a = applied("claude", "economy", "", "");
        assert_eq!((a.model.as_str(), a.effort.as_str()), ("sonnet", "low"));
        assert_eq!(
            env(&a),
            vec![
                ("CLAUDE_CODE_SUBAGENT_MODEL", "haiku"),
                ("CLAUDE_CODE_SUBAGENT_MODEL_FORCE", "1"),
            ]
        );
        let argv = get("claude").unwrap().launch(&Launch {
            model: &a.model,
            effort: &a.effort,
            session: Some("u1"),
            prompt: None,
            extra: &a.extra,
            auto_approve: false,
        });
        assert_eq!(
            argv,
            [
                "claude",
                "--session-id",
                "u1",
                "--model",
                "sonnet",
                "--effort",
                "low"
            ]
        );
        let chip = a.chip.unwrap();
        assert_eq!(
            (chip.name.as_str(), chip.model.as_str()),
            ("Economy", "sonnet")
        );
    }

    #[test]
    fn antigravity_effort_is_exposed_and_passed_to_the_cli() {
        let p = Profile {
            id: "agy-effort".into(),
            name: "Agy effort".into(),
            agents: BTreeMap::from([(
                "agy".into(),
                AgentCfg {
                    effort: "xhigh".into(),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };
        let a = apply(get("agy").unwrap(), Some(&p), "", "");
        assert_eq!(a.effort, "xhigh");
        assert_eq!(support(get("agy").unwrap()).effort, Some(Via::Flag));
        let argv = get("agy").unwrap().launch(&Launch {
            model: &a.model,
            effort: &a.effort,
            session: None,
            prompt: Some("do it"),
            extra: &a.extra,
            auto_approve: false,
        });
        assert_eq!(argv, ["agy", "--effort", "xhigh", "-i", "do it"]);
    }

    #[test]
    fn a_task_with_a_deleted_profile_falls_back_to_its_scope() {
        let d = tmp("stale");
        let mut s = store();
        s.scopes.remove("/w/gone");
        save(&d, &s).unwrap();
        let t = crate::tasks::Task {
            agent: "claude".into(),
            project: "/w/app".into(),
            profile: Some("deleted".into()),
            ..Default::default()
        };
        let a = for_task(&d, get("claude").unwrap(), &t);
        assert_eq!(a.chip.unwrap().id, "economy");
        let t = crate::tasks::Task {
            profile: Some("mine".into()),
            ..t
        };
        assert_eq!(for_task(&d, get("claude").unwrap(), &t).model, "haiku");
    }

    #[test]
    fn explicit_values_beat_the_profile() {
        let a = applied("claude", "max", "sonnet", "low");
        assert_eq!((a.model.as_str(), a.effort.as_str()), ("sonnet", "low"));
        assert_eq!(a.chip.unwrap().id, "max");
        let none = apply(get("claude").unwrap(), None, "", "");
        assert_eq!(none, Applied::default());
    }

    #[test]
    fn codex_subagent_overrides_select_embedded_mode() {
        let p = Profile {
            id: "codex".into(),
            name: "Codex".into(),
            builtin: false,
            agents: BTreeMap::from([(
                "codex".to_string(),
                AgentCfg {
                    subagent_model: "gpt-5.4-mini".into(),
                    ..Default::default()
                },
            )]),
        };
        let a = apply(get("codex").unwrap(), Some(&p), "", "");
        assert!(a.env.is_empty());
        let argv = get("codex").unwrap().launch(&Launch {
            model: &a.model,
            effort: &a.effort,
            session: None,
            prompt: None,
            extra: &a.extra,
            auto_approve: false,
        });
        assert_eq!(
            argv,
            [
                "codex",
                "--no-daemon",
                "-c",
                "agents.default_subagent_model=\"gpt-5.4-mini\"",
            ]
        );
    }

    #[test]
    fn codex_subagent_effort_override_selects_embedded_mode() {
        let p = Profile {
            id: "codex".into(),
            name: "Codex".into(),
            builtin: false,
            agents: BTreeMap::from([(
                "codex".to_string(),
                AgentCfg {
                    subagent_effort: "medium".into(),
                    ..Default::default()
                },
            )]),
        };
        let a = apply(get("codex").unwrap(), Some(&p), "", "");
        let argv = get("codex").unwrap().launch(&Launch {
            model: &a.model,
            effort: &a.effort,
            session: None,
            prompt: None,
            extra: &a.extra,
            auto_approve: false,
        });
        assert_eq!(
            argv,
            [
                "codex",
                "--no-daemon",
                "-c",
                "agents.default_subagent_reasoning_effort=\"medium\"",
            ]
        );
    }

    #[test]
    fn codex_reasoning_effort_selects_embedded_mode() {
        let a = applied("codex", "max", "", "");
        assert!(a.extra.is_empty());
        let argv = get("codex").unwrap().launch(&Launch {
            model: &a.model,
            effort: &a.effort,
            session: None,
            prompt: None,
            extra: &a.extra,
            auto_approve: false,
        });
        assert_eq!(
            argv,
            [
                "codex",
                "--no-daemon",
                "-m",
                "gpt-5.5",
                "-c",
                "model_reasoning_effort=xhigh",
            ]
        );
    }

    #[test]
    fn codex_model_only_profile_keeps_shared_server_mode() {
        let a = apply(get("codex").unwrap(), None, "gpt-5.4", "");
        assert!(a.extra.is_empty());
        let argv = get("codex").unwrap().launch(&Launch {
            model: &a.model,
            effort: &a.effort,
            session: None,
            prompt: None,
            extra: &a.extra,
            auto_approve: false,
        });
        assert_eq!(argv, ["codex", "-m", "gpt-5.4"]);
    }

    #[test]
    fn opencode_gets_inline_config() {
        let p = Profile {
            id: "oc".into(),
            name: "OC".into(),
            builtin: false,
            agents: BTreeMap::from([(
                "opencode".to_string(),
                AgentCfg {
                    model: "anthropic/claude-sonnet-4-6".into(),
                    small_model: "anthropic/claude-haiku-4-5".into(),
                    subagents: BTreeMap::from([(
                        "review".to_string(),
                        Sub {
                            model: "openai/gpt-5.4-mini".into(),
                            effort: String::new(),
                        },
                    )]),
                    ..Default::default()
                },
            )]),
        };
        let a = apply(get("opencode").unwrap(), Some(&p), "", "");
        assert_eq!(a.model, "anthropic/claude-sonnet-4-6");
        let (k, v) = &a.env[0];
        assert_eq!(k, "OPENCODE_CONFIG_CONTENT");
        let v: Value = serde_json::from_str(v).unwrap();
        assert_eq!(v["small_model"], "anthropic/claude-haiku-4-5");
        assert_eq!(v["agent"]["review"]["model"], "openai/gpt-5.4-mini");
    }

    #[test]
    fn unsupported_fields_are_dropped_or_rejected() {
        let a = applied("gemini", "economy", "", "high");
        assert_eq!(
            (a.model.as_str(), a.effort.as_str()),
            ("gemini-2.5-flash", "high")
        );
        assert!(a.env.is_empty() && a.extra.is_empty());
        let mut s = store();
        s.profiles[0].agents.insert(
            "gemini".into(),
            AgentCfg {
                effort: "low".into(),
                ..Default::default()
            },
        );
        assert!(validate(&s)
            .unwrap_err()
            .1
            .contains("effort is not supported"));
    }

    #[test]
    fn antigravity_named_subagents_use_supported_model_tiers() {
        let mut s = store();
        s.scopes.remove("/w/gone");
        s.profiles[0].agents.insert(
            "agy".into(),
            AgentCfg {
                effort: "xhigh".into(),
                subagents: BTreeMap::from([(
                    "reviewer".into(),
                    Sub {
                        model: "flash".into(),
                        ..Default::default()
                    },
                )]),
                ..Default::default()
            },
        );
        validate(&s).unwrap();
        s.profiles[0]
            .agents
            .get_mut("agy")
            .unwrap()
            .subagents
            .get_mut("reviewer")
            .unwrap()
            .model = "flash_lite".into();
        assert!(validate(&s)
            .unwrap_err()
            .1
            .contains("Antigravity subagent model must be"));
    }

    #[test]
    fn validation() {
        assert!(validate(&store()).is_err());
        let mut s = store();
        s.scopes.remove("/w/gone");
        validate(&s).unwrap();
        for bad in [
            |s: &mut Store| s.profiles[0].id = "economy".into(),
            |s: &mut Store| s.profiles[0].id = "a b".into(),
            |s: &mut Store| s.profiles[0].name = " ".into(),
            |s: &mut Store| s.default = "ghost".into(),
            |s: &mut Store| {
                s.scopes.insert("relative/path".into(), "max".into());
            },
            |s: &mut Store| s.profiles.push(s.profiles[0].clone()),
            |s: &mut Store| {
                s.profiles[0].agents.get_mut("claude").unwrap().model = "opus\nx: y".into()
            },
            |s: &mut Store| s.profiles[0].agents.get_mut("claude").unwrap().effort = "huge".into(),
            |s: &mut Store| {
                s.profiles[0]
                    .agents
                    .insert("shell".into(), AgentCfg::default());
            },
            |s: &mut Store| {
                s.profiles[0].agents.insert(
                    "codex".into(),
                    AgentCfg {
                        subagent_force: true,
                        ..Default::default()
                    },
                );
            },
        ] {
            let mut x = s.clone();
            bad(&mut x);
            assert!(validate(&x).is_err(), "{x:?}");
        }
    }

    #[test]
    fn save_and_load_keep_builtins_out_of_the_file() {
        let d = tmp("store");
        let mut s = store();
        s.scopes.remove("/w/gone");
        s.profiles[0]
            .agents
            .insert("codex".into(), AgentCfg::default());
        save(&d, &s).unwrap();
        let back = load(&d);
        assert_eq!(back.profiles.len(), 1);
        assert!(!back.profiles[0].agents.contains_key("codex"));
        let text = std::fs::read_to_string(path(&d)).unwrap();
        assert!(!text.contains("Economy") && !text.contains("builtin"));
        assert_eq!(to_json(&back)["profiles"].as_array().unwrap().len(), 4);
        assert!(to_json(&back)["support"]["claude"]["subagents"] == "file");
        assert_eq!(to_json(&back)["support"]["gemini"]["subagents"], "file");
        assert_eq!(to_json(&back)["support"]["agy"]["subagents"], "file");
        assert_eq!(to_json(&back)["support"]["agy"]["effort"], "flag");
        assert!(to_json(&back)["support"].get("shell").is_none());
    }

    const AGENT: &str = "---\nname: reviewer\ndescription: \"Reviews: code\"\nmodel: opus\ntools: Read, Grep\n---\n\nYou review code.\nmodel: not frontmatter\n";

    #[test]
    fn frontmatter_edits() {
        assert_eq!(front_get(AGENT, "model").as_deref(), Some("opus"));
        assert_eq!(
            front_get(AGENT, "description").as_deref(),
            Some("Reviews: code")
        );
        assert_eq!(front_get(AGENT, "effort"), None);
        let t = front_set(AGENT, "model", "haiku").unwrap();
        assert_eq!(t, AGENT.replace("model: opus", "model: haiku"));
        let t = front_set(&t, "effort", "low").unwrap();
        assert!(t.contains("tools: Read, Grep\neffort: low\n---\n"));
        assert!(t.ends_with("model: not frontmatter\n"));
        let t = front_set(&t, "effort", "").unwrap();
        assert_eq!(t, AGENT.replace("model: opus", "model: haiku"));
        let crlf = AGENT.replace('\n', "\r\n");
        let t = front_set(&crlf, "model", "sonnet").unwrap();
        assert_eq!(t, crlf.replace("model: opus", "model: sonnet"));
        assert_eq!(front_set("no front matter\n", "model", "x"), None);
        assert_eq!(front_set("---\nname: x\n", "model", "x"), None);
    }

    fn dirs(root: &Path) -> Dirs {
        Dirs {
            claude: root.join("home/.claude"),
            gemini: root.join("home/.gemini"),
            opencode: root.join("home/.config/opencode"),
        }
    }

    fn write(p: PathBuf, text: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    #[test]
    fn subagents_project_overrides_user() {
        let d = tmp("subs");
        let ds = dirs(&d);
        write(ds.claude.join("agents/reviewer.md"), AGENT);
        write(
            ds.claude.join("agents/planner.md"),
            "---\nname: planner\nmodel: sonnet\neffort: high\n---\nplan\n",
        );
        write(
            d.join("proj/.claude/agents/rev.md"),
            &AGENT.replace("model: opus", "model: haiku"),
        );
        write(
            ds.opencode.join("opencode.json"),
            r#"{"agent":{"build":{"model":"anthropic/x"},"plan":{}}}"#,
        );
        write(ds.claude.join("agents/notes.txt"), "ignored");
        write(
            ds.gemini.join("agents/reviewer.md"),
            "---\nname: reviewer\nmodel: gemini-3-flash-preview\n---\nreview\n",
        );
        write(
            ds.gemini.join("config/agents/security/agent.md"),
            "---\nname: security\nmodel: pro\n---\nsecurity review\n",
        );
        write(
            d.join("proj/.gemini/agents/planner.md"),
            "---\nname: planner\nmodel: gemini-3-pro-preview\n---\nplan\n",
        );
        write(
            d.join("proj/.agents/agents/implementer/agent.md"),
            "---\nname: implementer\nmodel: flash\n---\nimplement\n",
        );
        let all = subagents(&ds, Some(&d.join("proj")));
        let names: Vec<(&str, &str, &str, &str)> = all
            .iter()
            .map(|s| (s.agent, s.name.as_str(), s.scope, s.model.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                ("claude", "planner", "user", "sonnet"),
                ("claude", "reviewer", "project", "haiku"),
                ("gemini", "planner", "project", "gemini-3-pro-preview"),
                ("gemini", "reviewer", "user", "gemini-3-flash-preview"),
                ("agy", "implementer", "project", "flash"),
                ("agy", "security", "user", "pro"),
                ("opencode", "build", "user", "anthropic/x"),
                ("opencode", "plan", "user", ""),
            ]
        );
        assert_eq!(all[0].effort, "high");
        assert_eq!(subagents(&ds, None)[1].scope, "user");
    }

    #[test]
    fn subagent_profiles_update_gemini_and_antigravity_model_frontmatter() {
        let d = tmp("provider-subs");
        let ds = dirs(&d);
        let gemini = ds.gemini.join("agents/researcher.md");
        let agy = ds.gemini.join("config/agents/reviewer/agent.md");
        write(
            gemini.clone(),
            "---\nname: researcher\nmodel: gemini-2.5-flash\n---\nresearch\n",
        );
        write(
            agy.clone(),
            "---\nname: reviewer\nmodel: flash\n---\nreview\n",
        );
        let defs = subagents(&ds, None);
        let gemini_plan = plan_files(
            &defs,
            &AgentCfg {
                subagents: BTreeMap::from([(
                    "researcher".into(),
                    Sub {
                        model: "gemini-2.5-pro".into(),
                        effort: String::new(),
                    },
                )]),
                ..Default::default()
            },
        );
        assert_eq!(gemini_plan.len(), 1);
        assert_eq!(
            gemini_plan[0].changes,
            [(
                "model".into(),
                "gemini-2.5-flash".into(),
                "gemini-2.5-pro".into()
            )]
        );

        let agy_plan = plan_files(
            &defs,
            &AgentCfg {
                subagents: BTreeMap::from([(
                    "reviewer".into(),
                    Sub {
                        model: "pro".into(),
                        effort: String::new(),
                    },
                )]),
                ..Default::default()
            },
        );
        assert_eq!(agy_plan.len(), 1);
        assert_eq!(agy_plan[0].path, agy);
        assert_eq!(
            agy_plan[0].changes,
            [("model".into(), "flash".into(), "pro".into())]
        );
        assert!(std::fs::read_to_string(&gemini)
            .unwrap()
            .contains("model: gemini-2.5-flash"));
    }

    #[test]
    fn apply_files_dry_run_then_write_with_backup() {
        let d = tmp("apply");
        let ds = dirs(&d);
        let user = ds.claude.join("agents/reviewer.md");
        write(user.clone(), AGENT);
        write(
            ds.claude.join("agents/planner.md"),
            "---\nname: planner\nmodel: haiku\n---\nplan\n",
        );
        let c = AgentCfg {
            subagents: BTreeMap::from([
                (
                    "reviewer".to_string(),
                    Sub {
                        model: "haiku".into(),
                        effort: "low".into(),
                    },
                ),
                (
                    "planner".to_string(),
                    Sub {
                        model: "haiku".into(),
                        effort: String::new(),
                    },
                ),
                (
                    "missing".to_string(),
                    Sub {
                        model: "opus".into(),
                        effort: String::new(),
                    },
                ),
            ]),
            ..Default::default()
        };
        let plan = plan_files(&subagents(&ds, None), &c);
        assert_eq!(plan.len(), 1);
        assert_eq!(
            plan[0].changes,
            [
                ("model".to_string(), "opus".to_string(), "haiku".to_string()),
                ("effort".to_string(), String::new(), "low".to_string()),
            ]
        );
        assert_eq!(std::fs::read_to_string(&user).unwrap(), AGENT);
        let bak = write_file(&plan[0], "202610291200").unwrap();
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), AGENT);
        assert!(bak
            .to_string_lossy()
            .ends_with("reviewer.md.bak-202610291200"));
        let now = std::fs::read_to_string(&user).unwrap();
        assert!(now.contains("model: haiku\n") && now.contains("effort: low\n"));
        assert!(plan_files(&subagents(&ds, None), &c).is_empty());
    }
}
