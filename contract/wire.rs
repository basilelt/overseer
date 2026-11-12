pub const PROTOCOL: u32 = 1;

pub const MUX_BANNER: &[u8] = b"SB:MUX 1\n";
pub const MUX_OPEN: u8 = 1;
pub const MUX_OPEN_OK: u8 = 2;
pub const MUX_OPEN_ERR: u8 = 3;
pub const MUX_DATA: u8 = 4;
pub const MUX_WINDOW: u8 = 5;
pub const MUX_EOF: u8 = 6;
pub const MUX_CLOSE: u8 = 7;
pub const MUX_PING: u8 = 8;
pub const MUX_PONG: u8 = 9;

pub const CMD_RESUME: &str = "resume";
pub const CMD_HOLD: &str = "hold";
pub const CMD_CLOSE: &str = "close";
pub const CMD_LINK: &str = "link";
pub const CMD_RELAUNCH: &str = "relaunch";
pub const CMD_START: &str = "start";
pub const RELAY_COMMANDS: [&str; 5] = [CMD_RESUME, CMD_HOLD, CMD_CLOSE, CMD_LINK, CMD_RELAUNCH];

pub const STATUS_PENDING: &str = "pending";
pub const STATUS_DELIVERED: &str = "delivered";
pub const STATUS_SUCCEEDED: &str = "succeeded";
pub const STATUS_REFUSED: &str = "refused";
pub const STATUS_FAILED: &str = "failed";
pub const STATUS_TERMINAL: [&str; 3] = [STATUS_SUCCEEDED, STATUS_REFUSED, STATUS_FAILED];

pub const FRAME_HELLO: &str = "hello";
pub const FRAME_SNAPSHOT: &str = "snapshot";
pub const FRAME_DELTA: &str = "delta";
pub const FRAME_ACK: &str = "ack";
pub const FRAME_HEARTBEAT: &str = "heartbeat";
pub const FRAME_COMMAND: &str = "command";
pub const FRAME_COMMAND_RESULT: &str = "command_result";
pub const FRAME_STATIONS: &str = "stations";
pub const FRAME_RESYNC: &str = "resync";

pub const STATE_ACTIVE: &str = "active";
pub const STATE_IDLE: &str = "idle";
pub const STATE_HELD: &str = "held";
pub const STATE_HUNG_UP: &str = "hung-up";
pub const LINE_STATES: [&str; 4] = [STATE_ACTIVE, STATE_IDLE, STATE_HELD, STATE_HUNG_UP];

pub const STATE_DIR_NAME: &str = "overseer";
pub const SOCKET_FILE: &str = "daemon.sock";
pub const ENDPOINT_FILE: &str = "daemon.json";
