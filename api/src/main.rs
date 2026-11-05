use overseer_api::{router, util::now_ms, App};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

fn env(k: &str, default: &str) -> String {
    std::env::var(k)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.into())
}

#[tokio::main]
async fn main() -> ExitCode {
    let addr = env("OVERSEER_API_ADDR", "0.0.0.0:8080");
    let db = env("OVERSEER_API_DB", "overseer-api.db");
    let admin = std::env::var("OVERSEER_ADMIN_TOKEN").ok();
    if admin.as_deref().is_none_or(str::is_empty) {
        eprintln!("overseer-api: OVERSEER_ADMIN_TOKEN unset, admin endpoints disabled");
    }
    let app = match App::open(db.as_ref(), admin.as_deref(), Arc::new(now_ms)) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("overseer-api: {e}");
            return ExitCode::FAILURE;
        }
    };
    app.start(Some(Duration::from_secs(5)));
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("overseer-api: cannot listen on {addr}: {e}");
            return ExitCode::FAILURE;
        }
    };
    if let Ok(a) = listener.local_addr() {
        println!("overseer-api listening on {a}");
    }
    let stop = async {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = term.recv() => {},
        }
    };
    tokio::select! {
        r = axum::serve(listener, router(app)) => {
            if let Err(e) = r {
                eprintln!("overseer-api: {e}");
                return ExitCode::FAILURE;
            }
        }
        _ = stop => {}
    }
    ExitCode::SUCCESS
}
