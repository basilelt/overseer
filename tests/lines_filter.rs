#![cfg(target_os = "linux")]
mod common;

use common::Fx;
use serde_json::Value;

fn lines(fx: &Fx, query: &str) -> Vec<Value> {
    let (code, body) = fx.req("GET", &format!("/v1/lines{query}"), "");
    assert_eq!(code, 200, "{body}");
    serde_json::from_str(&body).unwrap()
}

#[test]
fn a_ticket_key_filter_keeps_only_that_ticket_and_an_unrecognised_one_keeps_nothing() {
    let fx = Fx::new("lines-filter", 2);
    let _d = fx.daemon(false);
    let all = lines(&fx, "?all=true");
    assert_eq!(all.len(), 2, "{all:?}");
    let ticket = all[0]["line"]["ticketKey"].as_str().unwrap().to_string();
    assert_eq!(
        lines(&fx, &format!("?all=true&ticketKey={ticket}")).len(),
        2
    );
    assert!(lines(&fx, "?all=true&ticketKey=AS-9999").is_empty());
    assert!(lines(&fx, "?all=true&ticketKey=nonsense").is_empty());
    assert!(lines(&fx, "?all=true&ticketKey=%21%21").is_empty());
}
