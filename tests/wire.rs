use overseer::wire as daemon;
use overseer_api::wire as relay;

#[test]
fn the_daemon_and_the_relay_share_one_vocabulary() {
    assert_eq!(daemon::RELAY_COMMANDS, relay::RELAY_COMMANDS);
    assert_eq!(daemon::STATUS_TERMINAL, relay::STATUS_TERMINAL);
    assert_eq!(daemon::LINE_STATES, relay::LINE_STATES);
    assert_eq!(daemon::FRAME_COMMAND_RESULT, relay::FRAME_COMMAND_RESULT);
}

#[test]
fn wire_strings_keep_their_protocol_values() {
    assert_eq!(
        daemon::RELAY_COMMANDS,
        ["resume", "hold", "close", "link", "relaunch"]
    );
    assert_eq!(daemon::CMD_START, "start");
    assert_eq!(daemon::STATUS_TERMINAL, ["succeeded", "refused", "failed"]);
    assert_eq!(daemon::STATUS_PENDING, "pending");
    assert_eq!(daemon::STATUS_DELIVERED, "delivered");
    assert_eq!(
        [
            daemon::FRAME_HELLO,
            daemon::FRAME_SNAPSHOT,
            daemon::FRAME_DELTA,
            daemon::FRAME_ACK,
            daemon::FRAME_HEARTBEAT,
            daemon::FRAME_COMMAND,
            daemon::FRAME_COMMAND_RESULT,
            daemon::FRAME_STATIONS,
            daemon::FRAME_RESYNC,
        ],
        [
            "hello",
            "snapshot",
            "delta",
            "ack",
            "heartbeat",
            "command",
            "command_result",
            "stations",
            "resync"
        ]
    );
    assert_eq!(daemon::LINE_STATES, ["active", "idle", "held", "hung-up"]);
    assert_eq!(daemon::PROTOCOL, 1);
    assert_eq!(daemon::MUX_BANNER, b"SB:MUX 1\n");
    assert_eq!(
        [
            daemon::MUX_OPEN,
            daemon::MUX_OPEN_OK,
            daemon::MUX_OPEN_ERR,
            daemon::MUX_DATA,
            daemon::MUX_WINDOW,
            daemon::MUX_EOF,
            daemon::MUX_CLOSE,
            daemon::MUX_PING,
            daemon::MUX_PONG
        ],
        [1, 2, 3, 4, 5, 6, 7, 8, 9]
    );
}

#[test]
fn state_directory_names_keep_their_values() {
    assert_eq!(daemon::STATE_DIR_NAME, "overseer");
    assert_eq!(daemon::SOCKET_FILE, "daemon.sock");
    assert_eq!(daemon::ENDPOINT_FILE, "daemon.json");
    assert_eq!(relay::STATE_DIR_NAME, daemon::STATE_DIR_NAME);
}
