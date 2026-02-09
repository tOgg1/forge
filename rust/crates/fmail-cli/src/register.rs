use crate::{CommandOutput, FmailBackend};

pub fn run_register_for_test(args: &[&str], backend: &dyn FmailBackend) -> CommandOutput {
    let mut json = false;
    let mut name: Option<String> = None;

    for arg in args {
        match *arg {
            "--json" => json = true,
            "" => {}
            v if v.starts_with("--") => {
                return CommandOutput {
                    stdout: String::new(),
                    stderr: format!("unknown flag: {v}\n"),
                    exit_code: 2,
                };
            }
            v => {
                if name.is_some() {
                    return CommandOutput {
                        stdout: String::new(),
                        stderr: "expected at most 1 argument\n".to_string(),
                        exit_code: 2,
                    };
                }
                name = Some(v.to_string());
            }
        }
    }

    let host = backend.hostname();

    let record = match name {
        Some(raw) => {
            let normalized = match fmail_core::validate::normalize_agent_name(&raw) {
                Ok(v) => v,
                Err(e) => {
                    return CommandOutput {
                        stdout: String::new(),
                        stderr: format!("invalid agent name: {e}\n"),
                        exit_code: 1,
                    };
                }
            };
            match backend.register_agent_record(&normalized, &host) {
                Ok(r) => r,
                Err(e) => {
                    if e == fmail_core::store::ERR_AGENT_EXISTS {
                        return CommandOutput {
                            stdout: String::new(),
                            stderr: format!("agent name already registered: {normalized}\n"),
                            exit_code: 1,
                        };
                    }
                    return CommandOutput {
                        stdout: String::new(),
                        stderr: format!("register agent: {e}\n"),
                        exit_code: 1,
                    };
                }
            }
        }
        None => {
            let mut rng = rand::thread_rng();
            match register_generated_agent(backend, &mut rng, &host) {
                Ok(r) => r,
                Err(e) => {
                    return CommandOutput {
                        stdout: String::new(),
                        stderr: format!("register agent: {e}\n"),
                        exit_code: 1,
                    };
                }
            }
        }
    };

    if json {
        let payload = match serde_json::to_string_pretty(&record) {
            Ok(v) => v,
            Err(e) => {
                return CommandOutput {
                    stdout: String::new(),
                    stderr: format!("encode agent: {e}\n"),
                    exit_code: 1,
                };
            }
        };
        return CommandOutput {
            stdout: format!("{payload}\n"),
            stderr: String::new(),
            exit_code: 0,
        };
    }

    CommandOutput {
        stdout: format!("{}\n", record.name.trim()),
        stderr: String::new(),
        exit_code: 0,
    }
}

const REGISTER_MAX_ATTEMPTS: usize = 10;

fn register_generated_agent<R: rand::Rng>(
    backend: &dyn FmailBackend,
    rng: &mut R,
    host: &str,
) -> Result<fmail_core::agent_registry::AgentRecord, String> {
    for _ in 0..REGISTER_MAX_ATTEMPTS {
        let candidate = fmail_core::names::random_loop_name_two_part(rng);
        match backend.register_agent_record(&candidate, host) {
            Ok(r) => return Ok(r),
            Err(e) if e == fmail_core::store::ERR_AGENT_EXISTS => continue,
            Err(e) => return Err(e),
        }
    }

    for _ in 0..REGISTER_MAX_ATTEMPTS {
        let candidate = fmail_core::names::random_loop_name_three_part(rng);
        match backend.register_agent_record(&candidate, host) {
            Ok(r) => return Ok(r),
            Err(e) if e == fmail_core::store::ERR_AGENT_EXISTS => continue,
            Err(e) => return Err(e),
        }
    }

    Err("unable to allocate unique agent name".to_string())
}
