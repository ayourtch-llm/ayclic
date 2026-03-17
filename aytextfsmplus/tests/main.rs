use aytextfsmplus::*;

#[cfg(test)]
mod tests {
    use super::*;
    use pest::Parser;

    #[test]
    fn test_regex_pattern() {
        let input = r#"((\d+\/?)+)
"#;
        let pairs = TextFSMPlusParser::parse(Rule::regex_pattern, input).unwrap();
        assert_eq!(pairs.count(), 1);
    }
    #[test]
    fn test_rule_with_err_msg() {
        let input = r#"  ^.* -> Error "Could not parse line:""#;
        let pairs = TextFSMPlusParser::parse(Rule::rule, input).unwrap();
        assert_eq!(pairs.count(), 1);
    }
    #[test]
    fn test_err_msg() {
        let input = r#""test""#;
        let pairs = TextFSMPlusParser::parse(Rule::err_msg, input).unwrap();
        assert_eq!(pairs.count(), 1);
    }
    #[test]
    fn test_value_definition() {
        let input = r#"Value PORT ((\d+\/?)+)
"#;
        let pairs = TextFSMPlusParser::parse(Rule::value_definition, input).unwrap();
        assert_eq!(pairs.count(), 1);
    }

    #[test]
    fn test_state_definition() {
        let input = "Start\n  ^interface -> Continue.Record End\n";
        let pairs = TextFSMPlusParser::parse(Rule::state_definition, input).unwrap();
        assert_eq!(pairs.count(), 1);
    }

    #[test]
    fn test_complete_template() {
        let input = r#"Value Required INTERFACE (.*)
Value DESCRIPTION (.*)

Start
  ^interface -> GetDescription
  ^$ -> Start

GetDescription
  ^description -> Continue.Record Start
  ^$ -> GetDescription
  ^. -> Error
"#;
        let pairs = TextFSMPlusParser::parse(Rule::file, input).unwrap();
        println!("Pairs: {:?}", &pairs);
        assert_eq!(pairs.count(), 3);
    }

    #[test]
    fn test_complete_template_asa() {
        let input = r#"Value Required RESOURCE (.+?)
Value DENIED (\d+)
Value CONTEXT (.+?)

Start
  ^x -> GetDescription

Startr_ecord
  ^x -> X
"#;
        let pairs = TextFSMPlusParser::parse(Rule::file, input).unwrap();
        println!("Pairs: {:?}", &pairs);
        assert_eq!(pairs.count(), 3);
    }

    #[test]
    fn test_complete_template_error() {
        let input = r#"Value PORT_ID (\S+)
Value DESCRIPTION (.+)

Start
  ^=+\s*$$
  ^\s*$$
  ^Port\s+Descriptions\s+on\s\S+\s+\S+\s*$$
  ^Port\s+Id\s+Description\s*$$
  ^${PORT_ID}\s+${DESCRIPTION}\s*$$ -> Record
  ^-+\s*$$
  ^. -> Error"#;
        let pairs = TextFSMPlusParser::parse(Rule::file, input).unwrap();
        println!("Pairs: {:?}", &pairs);
        assert_eq!(pairs.count(), 3);
    }

    #[test]
    fn test_rules_with_no_transitions() {
        let input = r#"Start
  ^interface$
  ^$
"#;
        let pairs = TextFSMPlusParser::parse(Rule::state_definition, input).unwrap();
        println!("Pairs: {:?}", &pairs);
        assert_eq!(pairs.count(), 1);
    }

    #[test]
    fn test_rules_with_no_transitions_complex() {
        let input = r#"Start
  ^PING\s+${DESTINATION}\s+${PKT_SIZE}\s+data\s+bytes*$$
  ^(?:${RESPONSE_STREAM})
  ^\.*$$
  ^\s*$$
  ^-+
  ^${SENT_QTY}\s+packet(?:s)?\s+transmitted,(?:\s+${BOUNCE_QTY}\s+packet(?:s)?\s+bounced,)?\s+${SUCCESS_QTY}\s+packet(?:s)?\s+received,\s+(?:${DUPLICATE_QTY}\s+duplicate(?:s)?)?(?:${LOSS_PCT}%\s+packet\s+loss)?
  ^(?:round-trip\s+min\s+=\s+${RTT_MIN}ms,\s+avg\s+=\s+${RTT_AVG}ms,\s+max\s+=\s+${RTT_MAX}ms,\s+stddev\s+=\s+${STD_DEV}ms)?
  # Error out if raw data does not match any above rules.
  ^.* -> Error "Could not parse line:"
"#;
        let pairs = TextFSMPlusParser::parse(Rule::state_definition, input).unwrap();
        println!("Pairs: {:?}", &pairs);
        assert_eq!(pairs.count(), 1);
    }

    #[test]
    fn test_rules_with_no_transitions_complex_error_nomsg() {
        let input = r#"Start
  ^PING\s+${DESTINATION}\s+${PKT_SIZE}\s+data\s+bytes*$$
  ^(?:${RESPONSE_STREAM})
  ^\.*$$
  ^\s*$$
  ^-+
  ^${SENT_QTY}\s+packet(?:s)?\s+transmitted,(?:\s+${BOUNCE_QTY}\s+packet(?:s)?\s+bounced,)?\s+${SUCCESS_QTY}\s+packet(?:s)?\s+received,\s+(?:${DUPLICATE_QTY}\s+duplicate(?:s)?)?(?:${LOSS_PCT}%\s+packet\s+loss)?
  ^(?:round-trip\s+min\s+=\s+${RTT_MIN}ms,\s+avg\s+=\s+${RTT_AVG}ms,\s+max\s+=\s+${RTT_MAX}ms,\s+stddev\s+=\s+${STD_DEV}ms)?
  # Error out if raw data does not match any above rules.
  ^.* -> Error
"#;
        let pairs = TextFSMPlusParser::parse(Rule::state_definition, input).unwrap();
        println!("Pairs: {:?}", &pairs);
        assert_eq!(pairs.count(), 1);
    }

    #[test]
    fn test_multiple_value_definitions() {
        let input = r#"Value HOSTNAME (.+)
Value VERSION (\d+\.\d+)
Value MODEL (\w+)

"#;
        let pairs = TextFSMPlusParser::parse(Rule::value_definitions, input).unwrap();
        assert_eq!(pairs.count(), 1);
    }

    #[test]
    fn test_complex_rule_nostate() {
        let input = r#"  ^PING\s+${DESTINATION}\s+${PKT_SIZE}\s+data\s+bytes*$$
"#;
        let pairs = TextFSMPlusParser::parse(Rule::rule, input).unwrap();
        assert_eq!(pairs.count(), 1);
    }

    #[test]
    fn test_complex_rule() {
        let input = "  ^interface GigabitEthernet -> Record Start\n";
        let pairs = TextFSMPlusParser::parse(Rule::rule, input).unwrap();
        assert_eq!(pairs.count(), 1);
    }

    #[test]
    fn test_multiple_states() {
        let input = r#"Start
  ^interface -> GetDescription

GetDescription
  ^description -> Record Start
  ^. -> Error
"#;
        let pairs = TextFSMPlusParser::parse(Rule::state_definitions, input).unwrap();
        assert_eq!(pairs.count(), 1);
    }
}

fn print_pair(indent: usize, pair: &Pair<'_, Rule>) {
    // println!("Debug: {:#?}", &pair);
    let spaces = " ".repeat(indent);
    println!("{}Rule:    {:?}", spaces, pair.as_rule());
    println!("{}Span:    {:?}", spaces, pair.as_span());
    println!("{}Text:    {}", spaces, pair.as_str());
    for p in pair.clone().into_inner() {
        print_pair(indent + 2, &p);
    }
}

fn main() {
    for arg in std::env::args().skip(1) {
        // println!("Reading file {}", &arg);
        let template = std::fs::read_to_string(&arg).expect("File read failed");
        let template = format!("{}\n", template);

        match TextFSMPlusParser::parse(Rule::file, &template) {
            Ok(pairs) => {
                for pair in pairs {
                    print_pair(0, &pair);
                }
            }
            Err(e) => panic!("file {} Error: {}", &arg, e),
        }
    }
}

#[cfg(test)]
mod extended_tests {
    use aytextfsmplus::*;

    #[test]
    fn test_preset_value_parsing() {
        let template = r#"
Value Preset Username ()
Value Preset Password ()
Value Hostname (\S+)

Start
  ^Username:\s* -> Done
"#;
        let fsm = TextFSMPlus::from_str(template);
        assert!(fsm.parser.values.get("Username").unwrap().is_preset);
        assert!(fsm.parser.values.get("Password").unwrap().is_preset);
        assert!(!fsm.parser.values.get("Hostname").unwrap().is_preset);
    }

    #[test]
    fn test_set_preset_value() {
        let template = r#"
Value Preset Username ()
Value Hostname (\S+)

Start
  ^${Hostname}# -> Done
"#;
        let mut fsm = TextFSMPlus::from_str(template);
        fsm.set_preset("Username", "admin");
        assert_eq!(
            fsm.curr_record.get("Username"),
            Some(&Value::Single("admin".to_string()))
        );
    }

    #[test]
    #[should_panic(expected = "not declared as Preset")]
    fn test_set_preset_on_non_preset_panics() {
        let template = r#"
Value Hostname (\S+)

Start
  ^${Hostname}# -> Done
"#;
        let mut fsm = TextFSMPlus::from_str(template);
        fsm.set_preset("Hostname", "router1");
    }

    #[test]
    fn test_with_preset_builder() {
        let template = r#"
Value Preset Username ()
Value Preset Password ()

Start
  ^. -> Done
"#;
        let fsm = TextFSMPlus::from_str(template)
            .with_preset("Username", "admin")
            .with_preset("Password", "secret");
        assert_eq!(
            fsm.curr_record.get("Username"),
            Some(&Value::Single("admin".to_string()))
        );
        assert_eq!(
            fsm.curr_record.get("Password"),
            Some(&Value::Single("secret".to_string()))
        );
    }

    #[test]
    fn test_done_state_parsing() {
        let template = r#"
Value Hostname (\S+)

Start
  ^${Hostname}# -> Done
  ^. -> Error
"#;
        let fsm = TextFSMPlusParser::from_str(template);
        let start = fsm.states.get("Start").unwrap();
        match &start.rules[0].transition.line_action {
            LineAction::Next(Some(NextState::Done)) => {} // expected
            other => panic!("Expected Next(Some(Done)), got {:?}", other),
        }
    }

    #[test]
    fn test_done_stops_parsing() {
        let template = r#"
Value Hostname (\S+)

Start
  ^${Hostname}# -> Done
"#;
        let mut fsm = TextFSMPlus::from_str(template);
        let result = fsm.parse_line("Router1#");
        assert_eq!(result, Some(NextState::Done));
    }

    #[test]
    fn test_send_action_parsing() {
        let template = r#"
Value Preset Username ()

Start
  ^Username:\s* -> Send ${Username} WaitPassword
  ^# -> Done
"#;
        let fsm = TextFSMPlusParser::from_str(template);
        let start = fsm.states.get("Start").unwrap();
        match &start.rules[0].transition.line_action {
            LineAction::Send(text, Some(NextState::NamedState(state))) => {
                assert_eq!(text, "${Username}");
                assert_eq!(state, "WaitPassword");
            }
            other => panic!("Expected Send with next state, got {:?}", other),
        }
    }

    #[test]
    fn test_send_action_no_next_state() {
        let template = r#"
Value Preset Password ()

Start
  ^Password:\s* -> Send ${Password}
  ^# -> Done
"#;
        let fsm = TextFSMPlusParser::from_str(template);
        let start = fsm.states.get("Start").unwrap();
        match &start.rules[0].transition.line_action {
            LineAction::Send(text, None) => {
                assert_eq!(text, "${Password}");
            }
            other => panic!("Expected Send without next state, got {:?}", other),
        }
    }

    #[test]
    fn test_send_treated_as_next_in_parse_mode() {
        let template = r#"
Value Preset Username ()
Value Hostname (\S+)

Start
  ^Username:\s* -> Send ${Username} WaitPrompt

WaitPrompt
  ^${Hostname}# -> Done
"#;
        let mut fsm = TextFSMPlus::from_str(template)
            .with_preset("Username", "admin");
        // In parse mode, Send acts like Next — transitions to WaitPrompt
        let result = fsm.parse_line("Username: ");
        assert_eq!(
            result,
            Some(NextState::NamedState("WaitPrompt".to_string()))
        );
    }

    #[test]
    fn test_from_str_equivalence() {
        let template = r#"Value Required Interface (\S+)
Value Status (up|down)

Start
  ^${Interface}\s+is\s+${Status} -> Record
"#;
        let mut fsm = TextFSMPlus::from_str(template);
        fsm.parse_line("Gi0/1 is up");
        assert_eq!(fsm.records.len(), 1);
        assert_eq!(
            fsm.records[0].get("Interface"),
            Some(&Value::Single("Gi0/1".to_string()))
        );
    }

    #[test]
    fn test_expand_send_text_simple_variable() {
        let template = r#"
Value Preset Username ()

Start
  ^Username:\s* -> Send ${Username} Done
"#;
        let fsm = TextFSMPlus::from_str(template)
            .with_preset("Username", "admin");
        let expanded = fsm.expand_send_text("${Username}", &aytextfsmplus::NoFuncs);
        assert_eq!(expanded, "admin");
    }

    #[test]
    fn test_expand_send_text_quoted_literal() {
        let template = r#"
Value Preset Username ()

Start
  ^> -> Send "enable" Done
"#;
        let fsm = TextFSMPlus::from_str(template);
        let expanded = fsm.expand_send_text("\"enable\"", &aytextfsmplus::NoFuncs);
        assert_eq!(expanded, "enable");
    }

    #[test]
    fn test_expand_send_text_with_aycalc_string_concat() {
        let template = r#"
Value Preset A ()
Value Preset B ()

Start
  ^calc -> Done
"#;
        let mut fsm = TextFSMPlus::from_str(template);
        fsm.set_preset("A", "hello");
        fsm.set_preset("B", "world");
        // String values get concatenated by aycalc
        let expanded = fsm.expand_send_text("${A + B}", &aytextfsmplus::NoFuncs);
        assert_eq!(expanded, "helloworld");
    }

    #[test]
    fn test_expand_send_text_with_aycalc_arithmetic() {
        let template = r#"
Value Preset A ()

Start
  ^calc -> Done
"#;
        let mut fsm = TextFSMPlus::from_str(template);
        fsm.set_preset("A", "10");
        // Pure arithmetic works when not involving string variables
        let expanded = fsm.expand_send_text("${2 + 40}", &aytextfsmplus::NoFuncs);
        assert_eq!(expanded, "42");
    }

    #[test]
    fn test_interactive_action_send() {
        let template = r#"
Value Preset Username ()

Start
  ^Username:\s* -> Send ${Username} WaitPassword

WaitPassword
  ^Password:\s* -> Done
"#;
        let mut fsm = TextFSMPlus::from_str(template)
            .with_preset("Username", "admin");
        let action = fsm.parse_line_interactive("Username: ", &aytextfsmplus::NoFuncs);
        assert_eq!(action, InteractiveAction::Send("admin".to_string()));
        assert_eq!(fsm.curr_state, "WaitPassword");
    }

    #[test]
    fn test_interactive_action_done() {
        let template = r#"
Value Hostname (\S+)

Start
  ^${Hostname}# -> Done
"#;
        let mut fsm = TextFSMPlus::from_str(template);
        let action = fsm.parse_line_interactive("Router1#", &aytextfsmplus::NoFuncs);
        assert_eq!(action, InteractiveAction::Done);
        assert_eq!(fsm.curr_state, "Done");
    }

    #[test]
    fn test_interactive_action_error() {
        let template = r#"
Start
  ^% -> Error "auth failed"
  ^# -> Done
"#;
        let mut fsm = TextFSMPlus::from_str(template);
        let action = fsm.parse_line_interactive("% Login invalid", &aytextfsmplus::NoFuncs);
        assert_eq!(
            action,
            InteractiveAction::Error(Some("\"auth failed\"".to_string()))
        );
    }

    #[test]
    fn test_interactive_multi_state_flow() {
        let template = r#"
Value Preset Username ()
Value Preset Password ()
Value Hostname (\S+)

Start
  ^Username:\s* -> Send ${Username} WaitPassword

WaitPassword
  ^Password:\s* -> Send ${Password} WaitPrompt

WaitPrompt
  ^${Hostname}# -> Done
  ^% -> Error "login failed"
"#;
        let mut fsm = TextFSMPlus::from_str(template)
            .with_preset("Username", "admin")
            .with_preset("Password", "secret123");

        let action = fsm.parse_line_interactive("Username: ", &aytextfsmplus::NoFuncs);
        assert_eq!(action, InteractiveAction::Send("admin".to_string()));
        assert_eq!(fsm.curr_state, "WaitPassword");

        let action = fsm.parse_line_interactive("Password: ", &aytextfsmplus::NoFuncs);
        assert_eq!(action, InteractiveAction::Send("secret123".to_string()));
        assert_eq!(fsm.curr_state, "WaitPrompt");

        let action = fsm.parse_line_interactive("Router1#", &aytextfsmplus::NoFuncs);
        assert_eq!(action, InteractiveAction::Done);
    }

    #[test]
    fn test_interactive_capture_and_reuse() {
        let template = r#"
Value Hostname (\S+)

Start
  ^${Hostname}> -> Send "enable" Enable

Enable
  ^${Hostname}# -> Done
"#;
        let mut fsm = TextFSMPlus::from_str(template);

        let action = fsm.parse_line_interactive("Router1>", &aytextfsmplus::NoFuncs);
        assert_eq!(action, InteractiveAction::Send("enable".to_string()));
        assert_eq!(fsm.curr_state, "Enable");

        // Hostname was captured, now the Enable state should match it
        let action = fsm.parse_line_interactive("Router1#", &aytextfsmplus::NoFuncs);
        assert_eq!(action, InteractiveAction::Done);
    }

    #[test]
    fn test_full_interactive_template_parses() {
        let template = r#"
Value Preset Username ()
Value Preset Password ()
Value Preset EnableSecret ()
Value Hostname (\S+)

Start
  ^Username:\s* -> Send ${Username} WaitPassword
  ^Password:\s* -> Send ${Password} WaitPrompt

WaitPassword
  ^Password:\s* -> Send ${Password} WaitPrompt

WaitPrompt
  ^${Hostname}# -> Done
  ^${Hostname}> -> Send "enable" Enable
  ^% -> Error "login failed"

Enable
  ^Password:\s* -> Send ${EnableSecret} CheckEnable

CheckEnable
  ^${Hostname}# -> Done
  ^${Hostname}> -> Error "enable failed"
  ^% -> Error "enable auth failed"
"#;
        let fsm = TextFSMPlusParser::from_str(template);
        assert!(fsm.values.get("Username").unwrap().is_preset);
        assert!(fsm.values.get("Password").unwrap().is_preset);
        assert!(fsm.values.get("EnableSecret").unwrap().is_preset);
        assert!(!fsm.values.get("Hostname").unwrap().is_preset);
        assert!(fsm.states.get("Start").is_some());
        assert!(fsm.states.get("WaitPassword").is_some());
        assert!(fsm.states.get("WaitPrompt").is_some());
        assert!(fsm.states.get("Enable").is_some());
        assert!(fsm.states.get("CheckEnable").is_some());
    }
}
