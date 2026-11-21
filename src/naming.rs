pub const TITLE_MAX: usize = 60;

fn tag_name(s: &str) -> Option<&str> {
    let rest = s.strip_prefix('<')?;
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .unwrap_or(rest.len());
    let name = &rest[..end];
    if is_html_tag(name) {
        return None;
    }
    let next = rest[end..].chars().next();
    (name.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && matches!(next, Some('>' | ' ' | '\n' | '/')))
    .then_some(name)
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(name) = tag_name(rest) else {
            out.push('<');
            rest = &rest[1..];
            continue;
        };
        let close = format!("</{name}>");
        if let Some(j) = rest.find(&close) {
            rest = &rest[j + close.len()..];
            out.push('\n');
        } else if let Some(j) = rest.find('>').filter(|j| rest[..*j].ends_with('/')) {
            rest = &rest[j + 1..];
        } else {
            out.push('<');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

fn strip_placeholders(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('[') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let end = rest.find(']').filter(|j| !rest[..*j].contains('\n'));
        let inner = end.map(|j| &rest[1..j]).unwrap_or("");
        if ["Pasted text", "Image", "Pasted image", "File"]
            .iter()
            .any(|p| inner.starts_with(p))
        {
            rest = &rest[end.unwrap_or(0) + 1..];
        } else {
            out.push('[');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

fn is_slash_command(s: &str) -> bool {
    let Some(rest) = s.strip_prefix('/') else {
        return false;
    };
    let word = rest.split_whitespace().next().unwrap_or("");
    !word.is_empty()
        && !word.contains('/')
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':'))
}

pub fn clean_prompt(s: &str) -> Option<String> {
    let text = strip_placeholders(&strip_tags(s));
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim_end)
        .skip_while(|l| l.trim().is_empty())
        .collect();
    let first = lines.first()?.trim();
    if first.starts_with("Caveat:")
        || first.starts_with("# AGENTS.md instructions")
        || is_slash_command(first)
        || first.starts_with("This session is being continued from a previous conversation")
    {
        return None;
    }
    let out = lines.join("\n").trim().to_string();
    (!out.is_empty()).then_some(out)
}

fn is_path(w: &str) -> bool {
    let w = w.trim_matches(|c: char| matches!(c, '"' | '\'' | '`' | '(' | ')' | ',' | ';'));
    (w.starts_with('/') && w[1..].contains('/'))
        || w.starts_with("~/")
        || w.starts_with("./")
        || w.starts_with("../")
        || w.starts_with("file://")
        || w.starts_with("http://")
        || w.starts_with("https://")
}

fn first_sentence(s: &str) -> &str {
    let mut prev: Option<char> = None;
    for (i, c) in s.char_indices() {
        if c.is_whitespace() && matches!(prev, Some('.' | '!' | '?' | '…')) {
            return &s[..i];
        }
        prev = Some(c);
    }
    s
}

fn sentence_case(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

pub fn shorten(s: &str, max: usize) -> String {
    let flat = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let cut: String = flat.chars().take(max).collect();
    let cut = match cut.rfind(' ') {
        Some(i) if i > max / 2 => &cut[..i],
        _ => &cut[..],
    };
    format!(
        "{}…",
        cut.trim_end_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
    )
}

const HTML_TAGS: &[&str] = &[
    "a",
    "b",
    "i",
    "u",
    "s",
    "p",
    "br",
    "hr",
    "em",
    "strong",
    "code",
    "pre",
    "span",
    "div",
    "kbd",
    "sub",
    "sup",
    "small",
    "mark",
    "del",
    "ins",
    "li",
    "ul",
    "ol",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "img",
    "blockquote",
];

fn is_html_tag(name: &str) -> bool {
    HTML_TAGS.iter().any(|t| t.eq_ignore_ascii_case(name))
}

fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let body = rest.strip_prefix("</").unwrap_or(&rest[1..]);
        let name_end = body
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(body.len());
        let end = rest.find('>');
        match end {
            Some(e) if is_html_tag(&body[..name_end]) => {
                rest = &rest[e + 1..];
            }
            _ => {
                out.push('<');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn strip_heading(line: &str) -> &str {
    let t = line.trim_start_matches('#');
    match t.strip_prefix(' ') {
        Some(r) if t.len() < line.len() => r,
        _ => line,
    }
}

pub fn plain(s: &str) -> String {
    let s = strip_html(s);
    let s = s.lines().map(strip_heading).collect::<Vec<_>>().join("\n");
    let s = s.as_str();
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('[') {
        let (head, tail) = rest.split_at(i);
        let link = tail.find("](").and_then(|m| {
            let end = tail[m..].find(')')? + m;
            (!tail[1..m].contains('\n')).then_some((m, end))
        });
        match link {
            Some((m, end)) => {
                out.push_str(head.strip_suffix('!').unwrap_or(head));
                out.push_str(&tail[1..m]);
                rest = &tail[end + 1..];
            }
            None => {
                out.push_str(head);
                out.push('[');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out.replace("**", "").replace("__", "").replace('`', "")
}

pub fn title_from_prompt(s: &str) -> Option<String> {
    let clean = plain(&clean_prompt(s)?);
    let line = clean
        .lines()
        .map(|l| {
            l.split_whitespace()
                .filter(|w| !is_path(w))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .map(|l| {
            l.trim_start_matches(['#', '-', '*', '>', ' ', ':'])
                .to_string()
        })
        .find(|l| l.chars().any(char::is_alphanumeric))?;
    let sentence = first_sentence(&line)
        .trim()
        .trim_end_matches(['.', '!', ':', ';', ','])
        .trim();
    let title = shorten(sentence, TITLE_MAX);
    (!title.is_empty()).then(|| sentence_case(&title))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_drops_markdown_links_emphasis_and_code() {
        assert_eq!(
            plain("Here: [docs](http://x.y/z) and **bold** `code`"),
            "Here: docs and bold code"
        );
        assert_eq!(plain("![shot](a.png) [broken(link"), "shot [broken(link");
        assert_eq!(
            title_from_prompt("fix [the login](http://a.b/c) loop").as_deref(),
            Some("Fix the login loop")
        );
    }

    #[test]
    fn plain_drops_heading_markers_and_html_tags_keeping_inner_text() {
        let p = "## Fix **bold** `code` [link](http://x.y) <b>tag</b> now";
        assert_eq!(plain(p), "Fix bold code link tag now");
        assert_eq!(plain("# One\n### Two\n#nospace"), "One\nTwo\n#nospace");
        assert_eq!(plain("a < b and <br/> c"), "a < b and  c");
        assert_eq!(
            title_from_prompt(p).as_deref(),
            Some("Fix bold code link tag now")
        );
        assert_eq!(
            clean_prompt("<system-reminder>x</system-reminder>\nhi <em>there</em>").as_deref(),
            Some("hi <em>there</em>")
        );
    }

    #[test]
    fn strips_system_reminders_and_command_blocks() {
        let p = "<system-reminder>\nYou are in plan mode.\n</system-reminder>\nfix the login redirect loop";
        assert_eq!(
            clean_prompt(p).as_deref(),
            Some("fix the login redirect loop")
        );
        assert_eq!(
            title_from_prompt(p).as_deref(),
            Some("Fix the login redirect loop")
        );
        let cmd = "<command-name>/clear</command-name>\n<command-message>clear</command-message>\n<command-args></command-args>";
        assert_eq!(clean_prompt(cmd), None);
        let caveat = "<local-command-caveat>Caveat: the messages below were generated by the user</local-command-caveat>";
        assert_eq!(clean_prompt(caveat), None);
        assert_eq!(clean_prompt("Caveat: The messages below"), None);
        let env = "<environment_context>\n  <cwd>/repo</cwd>\n</environment_context>";
        assert_eq!(clean_prompt(env), None);
    }

    #[test]
    fn skips_slash_commands_but_keeps_paths() {
        assert_eq!(clean_prompt("/clear"), None);
        assert_eq!(clean_prompt("/code-review high"), None);
        assert_eq!(clean_prompt("/model opus"), None);
        assert!(clean_prompt("/home/me/shot.png what is wrong here").is_some());
    }

    #[test]
    fn drops_pasted_content_and_path_noise() {
        let p = "/home/alice/.local/state/overseer/uploads/20261029/694af0cb-Screenshot.png the naming on the kanban view is kinda shit how to improve this";
        assert_eq!(
            title_from_prompt(p).as_deref(),
            Some("The naming on the kanban view is kinda shit how to improve…")
        );
        let pasted = "[Pasted text #1 +42 lines] why does this panic? It worked yesterday.";
        assert_eq!(
            title_from_prompt(pasted).as_deref(),
            Some("Why does this panic?")
        );
        assert_eq!(
            title_from_prompt("[Image #1] [Image #2]\nalign the header with the sidebar")
                .as_deref(),
            Some("Align the header with the sidebar")
        );
        assert_eq!(title_from_prompt("[Image #1]"), None);
        assert_eq!(
            title_from_prompt("look at [the docs] please").as_deref(),
            Some("Look at [the docs] please")
        );
    }

    #[test]
    fn first_sentence_sentence_case_and_length() {
        assert_eq!(
            title_from_prompt("add a health endpoint. Then wire it in main.rs and test it.")
                .as_deref(),
            Some("Add a health endpoint")
        );
        assert_eq!(
            title_from_prompt("# Task\n\nrename daemon.rs helpers").as_deref(),
            Some("Task")
        );
        let long = "refactor the session task title chain so that every card on the board has a readable name";
        let t = title_from_prompt(long).unwrap();
        assert!(t.chars().count() <= TITLE_MAX + 1, "{t}");
        assert!(t.ends_with('…'));
        assert!(t.starts_with("Refactor the session task"));
    }

    #[test]
    fn french_text() {
        assert_eq!(
            title_from_prompt("élargis la colonne des tâches. Merci !").as_deref(),
            Some("Élargis la colonne des tâches")
        );
        assert_eq!(
            title_from_prompt("<system-reminder>x</system-reminder>\nça plante quand je redimensionne le terminal sur le téléphone, peux-tu regarder ?")
                .as_deref(),
            Some("Ça plante quand je redimensionne le terminal sur le…")
        );
    }

    #[test]
    fn keeps_comparisons_that_look_like_tags() {
        assert_eq!(
            clean_prompt("make sure a < b and c<d hold").as_deref(),
            Some("make sure a < b and c<d hold")
        );
        assert_eq!(
            clean_prompt("use <T> generics here").as_deref(),
            Some("use <T> generics here")
        );
    }
}
