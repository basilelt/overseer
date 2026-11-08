struct Spec {
    flags: &'static [&'static str],
    valued: &'static [&'static str],
    min: usize,
    max: usize,
}

const NONE: Spec = Spec {
    flags: &[],
    valued: &[],
    min: 0,
    max: 0,
};

fn spec(cmd: &str) -> Option<Spec> {
    Some(match cmd {
        "daemon" => Spec {
            flags: &["--once", "--detach", "--replace"],
            valued: &["--interval", "--http", "--takeover"],
            ..NONE
        },
        "status" | "version" => Spec {
            flags: &["--json"],
            ..NONE
        },
        "board" | "doctor" | "socket-path" => NONE,
        "uninstall" => Spec {
            flags: &["--apply"],
            ..NONE
        },
        "resume" => Spec {
            flags: &["--print", "--attach"],
            min: 1,
            max: 1,
            ..NONE
        },
        "open" => Spec {
            flags: &["--yes"],
            min: 1,
            max: 1,
            ..NONE
        },
        "patch" => Spec {
            flags: &["--local", "--print"],
            min: 1,
            max: 1,
            ..NONE
        },
        "link" => Spec {
            valued: &["--env"],
            min: 1,
            max: 2,
            ..NONE
        },
        "pin" => Spec {
            flags: &["--off"],
            min: 1,
            max: 1,
            ..NONE
        },
        "hold" | "close" => Spec {
            flags: &["--yes", "--json"],
            min: 1,
            max: 1,
            ..NONE
        },
        "stub" | "titles" => Spec {
            min: 1,
            max: 1,
            ..NONE
        },
        _ => return None,
    })
}

pub fn wants_help(cmd: &str, rest: &[String]) -> bool {
    matches!(cmd, "-h" | "--help" | "help")
        || rest.first().is_some_and(|a| a == "-h" || a == "--help")
        || (cmd == "daemon" && rest.iter().any(|a| a == "-h" || a == "--help"))
}

pub fn check(cmd: &str, rest: &[String]) -> Result<(), String> {
    let Some(s) = spec(cmd) else {
        return Ok(());
    };
    if cmd == "daemon" && rest.iter().any(|a| a == "--takeover-check") {
        let extra = rest
            .iter()
            .filter(|a| a.as_str() != "--takeover-check")
            .count();
        return if extra <= 1 && rest.len() <= 2 {
            Ok(())
        } else {
            Err("`daemon --takeover-check` takes at most one file".into())
        };
    }
    let mut positionals = 0;
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        if s.valued.contains(&a.as_str()) {
            if it.next().is_none() {
                return Err(format!("{a} needs a value"));
            }
        } else if a.starts_with('-') && a.len() > 1 {
            if !s.flags.contains(&a.as_str()) {
                return Err(format!("unknown option {a} for `{cmd}`"));
            }
        } else {
            positionals += 1;
        }
    }
    if positionals > s.max {
        return Err(format!("unexpected argument for `{cmd}`"));
    }
    if positionals < s.min {
        return Err(format!("missing argument for `{cmd}`"));
    }
    if cmd == "daemon" {
        if let Some(i) = rest.iter().position(|a| a == "--interval") {
            if rest[i + 1].parse::<u64>().is_err() {
                return Err("--interval needs a number of seconds".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn help_is_recognised_for_every_command() {
        assert!(wants_help("daemon", &a("--help")));
        assert!(wants_help("daemon", &a("--once -h")));
        assert!(wants_help("ls", &a("-h")));
        assert!(wants_help("--help", &[]));
        assert!(wants_help("help", &[]));
        assert!(!wants_help("ls", &a("--all")));
        assert!(!wants_help("hook", &a("claude Stop")));
    }

    #[test]
    fn daemon_accepts_its_documented_flags() {
        for ok in [
            "",
            "--once",
            "--interval 5",
            "--http 127.0.0.1:0",
            "--detach --replace",
            "--takeover /tmp/x.json",
            "--takeover-check",
            "--takeover-check f.json",
        ] {
            assert_eq!(check("daemon", &a(ok)), Ok(()), "{ok}");
        }
    }

    #[test]
    fn daemon_rejects_unknown_and_extra_arguments() {
        for bad in [
            "--bogus",
            "-x",
            "extra",
            "--interval",
            "--interval abc",
            "--http",
            "--takeover-check a b",
        ] {
            assert!(check("daemon", &a(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn other_commands_reject_unknown_flags_and_wrong_arity() {
        assert!(check("status", &a("--json")).is_ok());
        assert!(check("status", &a("--jsno")).is_err());
        assert!(check("status", &a("extra")).is_err());
        assert!(check("board", &a("--x")).is_err());
        assert!(check("doctor", &a("x")).is_err());
        assert!(check("socket-path", &a("--x")).is_err());
        assert!(check("version", &a("--json")).is_ok());
        assert!(check("resume", &a("L1 --print --attach")).is_ok());
        assert!(check("resume", &a("")).is_err());
        assert!(check("resume", &a("L1 L2")).is_err());
        assert!(check("resume", &a("L1 --nope")).is_err());
        assert!(check("link", &a("L1 AS-1")).is_ok());
        assert!(check("link", &a("L1 --env e")).is_ok());
        assert!(check("link", &a("L1 --env")).is_err());
        assert!(check("link", &a("L1 AS-1 x")).is_err());
        assert!(check("hold", &a("L1 --yes --json")).is_ok());
        assert!(check("close", &a("--yes")).is_err());
        assert!(check("pin", &a("L1 --off")).is_ok());
        assert!(check("open", &a("overseer://resume/x --yes")).is_ok());
        assert!(check("patch", &a("AS-1 --local --print")).is_ok());
        assert!(check("stub", &a("L1")).is_ok());
        assert!(check("titles", &a("L1 L2")).is_err());
    }

    #[test]
    fn commands_with_their_own_parsers_pass_through() {
        assert!(check("ls", &a("--all")).is_ok());
        assert!(check("hook", &a("claude Stop 1")).is_ok());
        assert!(check("mcp", &[]).is_ok());
    }
}
