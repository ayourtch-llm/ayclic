use thiserror::Error;

#[derive(Error, Debug)]
pub enum CiscoIosError {
    #[error("Telnet error: {0}")]
    Telnet(#[from] aytelnet::TelnetError),

    #[error("SSH error: {0}")]
    Ssh(#[from] ayssh::SshError),

    #[error("Invalid connection type for this operation: {0}")]
    InvalidConnectionType(String),

    #[error("Not connected")]
    NotConnected,

    #[error("MD5 verification failed: expected {expected}, got {actual}")]
    Md5Mismatch { expected: String, actual: String },

    #[error("Failed to parse MD5 from device output: {0}")]
    Md5ParseError(String),

    #[error("HTTP upload error: {0}")]
    HttpUploadError(String),

    #[error("Operation timed out ({} bytes accumulated)", accumulated.len())]
    Timeout { accumulated: Vec<u8> },

    #[error("Serial mismatch: expected {expected}, got {actual}")]
    SerialMismatch { expected: String, actual: String, show_version_output: String },

    #[error("Failed to parse serial from device output: {0}")]
    SerialParseError(String),
}
