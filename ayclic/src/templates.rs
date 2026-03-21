//! Built-in TextFSMPlus templates for common device interactions.
//!
//! These templates are used by convenience constructors like
//! `CiscoIosConn::new()` to drive login, enable mode, and
//! command prompt detection without hardcoding the interaction
//! logic in Rust.

/// Cisco IOS SSH login template.
///
/// Expects the SSH transport to handle protocol-level auth.
/// After SSH connect, waits for the device prompt and sends
/// `terminal length 0` to disable pagination.
///
/// Preset values: (none — SSH auth is protocol-level)
pub const CISCO_IOS_SSH_POST_LOGIN: &str = r#"Value Hostname (\S+)

Start
  ^.*# -> Send "terminal length 0" TermLen

TermLen
  ^.*# -> Done
"#;

/// Cisco IOS Telnet login template.
///
/// Handles username/password prompts, then sends `terminal length 0`.
///
/// Preset values: Username, Password
pub const CISCO_IOS_TELNET_LOGIN: &str = r#"Value Preset Username ()
Value Preset Password ()
Value Hostname (\S+)

Start
  ^[Uu]sername:\s* -> Send ${Username} WaitPassword
  ^[Pp]assword:\s* -> Send ${Password} WaitPrompt
  ^.*# -> Send "terminal length 0" TermLen
  ^.*> -> Send "terminal length 0" TermLen

WaitPassword
  ^[Pp]assword:\s* -> Send ${Password} WaitPrompt

WaitPrompt
  ^.*# -> Send "terminal length 0" TermLen
  ^.*> -> Send "terminal length 0" TermLen
  ^% -> Error "login failed"

TermLen
  ^.*# -> Done
  ^.*> -> Done
"#;

/// Cisco IOS command prompt template.
///
/// Detects `#` prompt as command completion. Handles common
/// interactive prompts (confirmation, yes/no) automatically.
pub const CISCO_IOS_PROMPT: &str = r#"Start
  ^.*# -> Done
  ^.*\]\?\s* -> Send ""
  ^\[confirm\] -> Send ""
  ^.*\(yes/no\)\??\s* -> Send "yes"
  ^.*\(y/n\)\??\s* -> Send "y"
"#;

#[cfg(test)]
mod tests {
    use aytextfsmplus::TextFSMPlus;

    #[test]
    fn test_cisco_ssh_post_login_parses() {
        let fsm = TextFSMPlus::from_str(super::CISCO_IOS_SSH_POST_LOGIN);
        assert!(fsm.parser.states.get("Start").is_some());
        assert!(fsm.parser.states.get("TermLen").is_some());
    }

    #[test]
    fn test_cisco_telnet_login_parses() {
        let fsm = TextFSMPlus::from_str(super::CISCO_IOS_TELNET_LOGIN);
        assert!(fsm.parser.states.get("Start").is_some());
        assert!(fsm.parser.states.get("WaitPassword").is_some());
        assert!(fsm.parser.states.get("WaitPrompt").is_some());
        assert!(fsm.parser.values.get("Username").unwrap().is_preset);
        assert!(fsm.parser.values.get("Password").unwrap().is_preset);
    }

    #[test]
    fn test_cisco_prompt_parses() {
        let fsm = TextFSMPlus::from_str(super::CISCO_IOS_PROMPT);
        assert!(fsm.parser.states.get("Start").is_some());
    }
}
