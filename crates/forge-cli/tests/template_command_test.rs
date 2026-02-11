#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::field_reassign_with_default
)]

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use forge_cli::template::{run_for_test, Template, TemplateBackend, TemplateVar};

#[derive(Debug)]
struct RecordingTemplateBackend {
    templates: Vec<Template>,
    user_dir: PathBuf,
    project_dir: PathBuf,
    enqueued: RefCell<Vec<(String, String)>>,
}

impl RecordingTemplateBackend {
    fn new() -> Self {
        Self {
            templates: vec![Template {
                name: "deploy".to_string(),
                description: "Deploy service".to_string(),
                message: "Deploy {{.service}} to {{.env}} with {{ .strategy }}.".to_string(),
                variables: vec![
                    TemplateVar {
                        name: "service".to_string(),
                        description: String::new(),
                        default: String::new(),
                        required: true,
                    },
                    TemplateVar {
                        name: "env".to_string(),
                        description: String::new(),
                        default: "staging".to_string(),
                        required: false,
                    },
                    TemplateVar {
                        name: "strategy".to_string(),
                        description: String::new(),
                        default: "rolling".to_string(),
                        required: false,
                    },
                ],
                tags: vec!["ops".to_string()],
                source: "/project/.forge/templates/deploy.yaml".to_string(),
            }],
            user_dir: PathBuf::from("/home/user/.config/forge/templates"),
            project_dir: PathBuf::from("/project/.forge/templates"),
            enqueued: RefCell::new(Vec::new()),
        }
    }

    fn last_message(&self) -> Option<String> {
        self.enqueued.borrow().last().map(|item| item.1.clone())
    }
}

impl TemplateBackend for RecordingTemplateBackend {
    fn load_templates(&self) -> Result<Vec<Template>, String> {
        Ok(self.templates.clone())
    }

    fn user_template_dir(&self) -> Result<PathBuf, String> {
        Ok(self.user_dir.clone())
    }

    fn project_template_dir(&self) -> Result<Option<PathBuf>, String> {
        Ok(Some(self.project_dir.clone()))
    }

    fn file_exists(&self, _path: &Path) -> bool {
        false
    }

    fn create_dir_all(&self, _path: &Path) -> Result<(), String> {
        Ok(())
    }

    fn write_file(&self, _path: &Path, _contents: &str) -> Result<(), String> {
        Ok(())
    }

    fn remove_file(&self, _path: &Path) -> Result<(), String> {
        Ok(())
    }

    fn open_editor(&self, _path: &Path) -> Result<(), String> {
        Ok(())
    }

    fn enqueue_template(
        &self,
        message: &str,
        agent_flag: &str,
    ) -> Result<(String, String), String> {
        self.enqueued
            .borrow_mut()
            .push((agent_flag.to_string(), message.to_string()));
        Ok((agent_flag.to_string(), "item-001".to_string()))
    }
}

#[test]
fn template_run_human_matches_golden_and_interpolates_message() {
    let backend = RecordingTemplateBackend::new();
    let out = run(
        &[
            "template",
            "run",
            "deploy",
            "--agent",
            "agent_oracle",
            "--var",
            "service=api",
            "--var",
            "env=prod",
        ],
        &backend,
    );
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty());
    assert_eq!(out.stdout, include_str!("golden/template/run_human.txt"));
    assert_eq!(
        backend.last_message().as_deref(),
        Some("Deploy api to prod with rolling.")
    );
}

#[test]
fn template_run_json_matches_golden_and_uses_default_values() {
    let backend = RecordingTemplateBackend::new();
    let out = run(
        &[
            "template",
            "--json",
            "run",
            "deploy",
            "--agent",
            "agent_oracle",
            "--var",
            "service=api",
        ],
        &backend,
    );
    assert_eq!(out.exit_code, 0);
    assert!(out.stderr.is_empty());
    assert_eq!(out.stdout, include_str!("golden/template/run_json.txt"));
    assert_eq!(
        backend.last_message().as_deref(),
        Some("Deploy api to staging with rolling.")
    );
}

fn run(args: &[&str], backend: &dyn TemplateBackend) -> forge_cli::template::CommandOutput {
    run_for_test(args, backend)
}
