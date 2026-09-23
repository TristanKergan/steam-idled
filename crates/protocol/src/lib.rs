pub mod config;
pub mod games;
pub mod ipc;
pub mod state;

pub use config::Config;
pub use games::{Game, GameDatabase};
pub use ipc::{
    recv_json, send_json, ActiveGameInfo, GameEntry, GamesPayload, IpcClient, IpcError,
    LogsPayload, Request, Response, StatusPayload,
};
pub use state::DaemonState;
