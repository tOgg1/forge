//! Sandbox and explicit grant enforcement for extension behaviors.

use crate::extension_actions::ExtensionPermission;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SandboxCapability {
    FilesystemRead,
    FilesystemWrite,
    ProcessSpawn,
}

impl SandboxCapability {
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::FilesystemRead => "filesystem-read",
            Self::FilesystemWrite => "filesystem-write",
            Self::ProcessSpawn => "process-spawn",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxGrant {
    pub extension_id: String,
    pub capability: SandboxCapability,
    pub scope: String,
    pub granted_by: String,
    pub reason: String,
    pub expires_at_epoch_s: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SandboxGrantRegistry {
    grants: Vec<SandboxGrant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxPolicy {
    pub require_explicit_grant_for_filesystem: bool,
    pub require_explicit_grant_for_process: bool,
    pub allowed_read_roots: Vec<String>,
    pub allowed_write_roots: Vec<String>,
    pub blocked_path_prefixes: Vec<String>,
    pub allowed_program_prefixes: Vec<String>,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self {
            require_explicit_grant_for_filesystem: true,
            require_explicit_grant_for_process: true,
            allowed_read_roots: vec!["./".to_owned()],
            allowed_write_roots: vec!["./".to_owned()],
            blocked_path_prefixes: vec!["/etc".to_owned(), "/private".to_owned()],
            allowed_program_prefixes: vec!["forge".to_owned(), "sv".to_owned(), "fmail".to_owned()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxIntent {
    RunPaletteCommand { command: String },
    ReadFile { path: String },
    WriteFile { path: String },
    SpawnProcess { program: String, args: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxRequest<'a> {
    pub extension_id: &'a str,
    pub permissions: &'a [ExtensionPermission],
    pub now_epoch_s: i64,
    pub intent: SandboxIntent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxAuditRecord {
    pub extension_id: String,
    pub intent: String,
    pub allowed: bool,
    pub reason: String,
    pub capability: Option<SandboxCapability>,
    pub matched_grant_scope: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxDecision {
    pub allowed: bool,
    pub reason: String,
    pub capability: Option<SandboxCapability>,
    pub matched_grant_scope: Option<String>,
    pub audit: SandboxAuditRecord,
}

impl SandboxGrantRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn grant(&mut self, mut grant: SandboxGrant) {
        grant.extension_id = normalize_id(&grant.extension_id);
        grant.scope = normalize_scope(&grant.scope);
        if grant.granted_by.trim().is_empty() {
            grant.granted_by = "unknown".to_owned();
        }
        if grant.reason.trim().is_empty() {
            grant.reason = "explicit extension sandbox grant".to_owned();
        }
        if grant.extension_id.is_empty() || grant.scope.is_empty() {
            return;
        }
        self.grants.push(grant);
    }

    #[must_use]
    pub fn matching_grant(
        &self,
        extension_id: &str,
        capability: SandboxCapability,
        subject: &str,
        now_epoch_s: i64,
    ) -> Option<&SandboxGrant> {
        let extension_id = normalize_id(extension_id);
        let subject = normalize_scope(subject);
        self.grants.iter().find(|grant| {
            grant.extension_id == extension_id
                && grant.capability == capability
                && !is_expired(grant.expires_at_epoch_s, now_epoch_s)
                && subject.starts_with(&grant.scope)
        })
    }
}

#[must_use]
pub fn evaluate_sandbox_request(
    request: &SandboxRequest<'_>,
    policy: &SandboxPolicy,
    grants: &SandboxGrantRegistry,
) -> SandboxDecision {
    match &request.intent {
        SandboxIntent::ReadFile { path } => evaluate_file_read(request, policy, grants, path),
        SandboxIntent::WriteFile { path } => evaluate_file_write(request, policy, grants, path),
        SandboxIntent::SpawnProcess { program, .. } => {
            evaluate_process_spawn(request, policy, grants, program)
        }
        SandboxIntent::RunPaletteCommand { command } => {
            evaluate_palette_command(request, policy, grants, command)
        }
    }
}

fn evaluate_palette_command(
    request: &SandboxRequest<'_>,
    policy: &SandboxPolicy,
    grants: &SandboxGrantRegistry,
    command: &str,
) -> SandboxDecision {
    let command = command.trim().to_ascii_lowercase();
    if command.starts_with("exec ") {
        if !request
            .permissions
            .contains(&ExtensionPermission::ExecuteShell)
        {
            return deny(
                request,
                format!("command {command:?} missing execute-shell permission"),
                Some(SandboxCapability::ProcessSpawn),
                None,
            );
        }
        let program = command
            .split_whitespace()
            .nth(1)
            .map(ToOwned::to_owned)
            .unwrap_or_default();
        return evaluate_process_spawn(request, policy, grants, &program);
    }
    if is_loop_control_command(&command)
        && !request
            .permissions
            .contains(&ExtensionPermission::ControlLoops)
    {
        return deny(
            request,
            format!("command {command:?} missing control-loops permission"),
            None,
            None,
        );
    }
    allow(request, format!("command {command:?} allowed"), None, None)
}

fn evaluate_file_read(
    request: &SandboxRequest<'_>,
    policy: &SandboxPolicy,
    grants: &SandboxGrantRegistry,
    path: &str,
) -> SandboxDecision {
    let path = normalize_scope(path);
    if path.is_empty() {
        return deny(
            request,
            "empty filesystem read path".to_owned(),
            Some(SandboxCapability::FilesystemRead),
            None,
        );
    }
    if is_blocked_path(policy, &path) {
        return deny(
            request,
            format!("filesystem read blocked by protected prefix: {path}"),
            Some(SandboxCapability::FilesystemRead),
            None,
        );
    }
    if !matches_allowed_root(&policy.allowed_read_roots, &path) {
        return deny(
            request,
            format!("filesystem read path outside allowed roots: {path}"),
            Some(SandboxCapability::FilesystemRead),
            None,
        );
    }
    if policy.require_explicit_grant_for_filesystem {
        let grant = grants.matching_grant(
            request.extension_id,
            SandboxCapability::FilesystemRead,
            &path,
            request.now_epoch_s,
        );
        if let Some(grant) = grant {
            return allow(
                request,
                format!("filesystem read allowed by explicit grant {}", grant.scope),
                Some(SandboxCapability::FilesystemRead),
                Some(grant.scope.clone()),
            );
        }
        return deny(
            request,
            format!("filesystem read requires explicit grant for {path}"),
            Some(SandboxCapability::FilesystemRead),
            None,
        );
    }
    allow(
        request,
        format!("filesystem read allowed for {path}"),
        Some(SandboxCapability::FilesystemRead),
        None,
    )
}

fn evaluate_file_write(
    request: &SandboxRequest<'_>,
    policy: &SandboxPolicy,
    grants: &SandboxGrantRegistry,
    path: &str,
) -> SandboxDecision {
    let path = normalize_scope(path);
    if path.is_empty() {
        return deny(
            request,
            "empty filesystem write path".to_owned(),
            Some(SandboxCapability::FilesystemWrite),
            None,
        );
    }
    if !request
        .permissions
        .contains(&ExtensionPermission::WriteState)
    {
        return deny(
            request,
            format!("filesystem write missing write-state permission: {path}"),
            Some(SandboxCapability::FilesystemWrite),
            None,
        );
    }
    if is_blocked_path(policy, &path) {
        return deny(
            request,
            format!("filesystem write blocked by protected prefix: {path}"),
            Some(SandboxCapability::FilesystemWrite),
            None,
        );
    }
    if !matches_allowed_root(&policy.allowed_write_roots, &path) {
        return deny(
            request,
            format!("filesystem write path outside allowed roots: {path}"),
            Some(SandboxCapability::FilesystemWrite),
            None,
        );
    }
    if policy.require_explicit_grant_for_filesystem {
        let grant = grants.matching_grant(
            request.extension_id,
            SandboxCapability::FilesystemWrite,
            &path,
            request.now_epoch_s,
        );
        if let Some(grant) = grant {
            return allow(
                request,
                format!("filesystem write allowed by explicit grant {}", grant.scope),
                Some(SandboxCapability::FilesystemWrite),
                Some(grant.scope.clone()),
            );
        }
        return deny(
            request,
            format!("filesystem write requires explicit grant for {path}"),
            Some(SandboxCapability::FilesystemWrite),
            None,
        );
    }
    allow(
        request,
        format!("filesystem write allowed for {path}"),
        Some(SandboxCapability::FilesystemWrite),
        None,
    )
}

fn evaluate_process_spawn(
    request: &SandboxRequest<'_>,
    policy: &SandboxPolicy,
    grants: &SandboxGrantRegistry,
    program: &str,
) -> SandboxDecision {
    let program = normalize_scope(program);
    if program.is_empty() {
        return deny(
            request,
            "empty process program".to_owned(),
            Some(SandboxCapability::ProcessSpawn),
            None,
        );
    }
    if !request
        .permissions
        .contains(&ExtensionPermission::ExecuteShell)
    {
        return deny(
            request,
            format!("process spawn missing execute-shell permission: {program}"),
            Some(SandboxCapability::ProcessSpawn),
            None,
        );
    }
    if !policy.allowed_program_prefixes.is_empty()
        && !policy
            .allowed_program_prefixes
            .iter()
            .map(|prefix| normalize_scope(prefix))
            .any(|prefix| !prefix.is_empty() && program.starts_with(&prefix))
    {
        return deny(
            request,
            format!("process spawn program outside allowlist: {program}"),
            Some(SandboxCapability::ProcessSpawn),
            None,
        );
    }
    if policy.require_explicit_grant_for_process {
        let grant = grants.matching_grant(
            request.extension_id,
            SandboxCapability::ProcessSpawn,
            &program,
            request.now_epoch_s,
        );
        if let Some(grant) = grant {
            return allow(
                request,
                format!("process spawn allowed by explicit grant {}", grant.scope),
                Some(SandboxCapability::ProcessSpawn),
                Some(grant.scope.clone()),
            );
        }
        return deny(
            request,
            format!("process spawn requires explicit grant for {program}"),
            Some(SandboxCapability::ProcessSpawn),
            None,
        );
    }
    allow(
        request,
        format!("process spawn allowed for {program}"),
        Some(SandboxCapability::ProcessSpawn),
        None,
    )
}

fn allow(
    request: &SandboxRequest<'_>,
    reason: String,
    capability: Option<SandboxCapability>,
    matched_grant_scope: Option<String>,
) -> SandboxDecision {
    sandbox_decision(request, true, reason, capability, matched_grant_scope)
}

fn deny(
    request: &SandboxRequest<'_>,
    reason: String,
    capability: Option<SandboxCapability>,
    matched_grant_scope: Option<String>,
) -> SandboxDecision {
    sandbox_decision(request, false, reason, capability, matched_grant_scope)
}

fn sandbox_decision(
    request: &SandboxRequest<'_>,
    allowed: bool,
    reason: String,
    capability: Option<SandboxCapability>,
    matched_grant_scope: Option<String>,
) -> SandboxDecision {
    let audit = SandboxAuditRecord {
        extension_id: normalize_id(request.extension_id),
        intent: render_intent(&request.intent),
        allowed,
        reason: reason.clone(),
        capability,
        matched_grant_scope: matched_grant_scope.clone(),
    };
    SandboxDecision {
        allowed,
        reason,
        capability,
        matched_grant_scope,
        audit,
    }
}

fn render_intent(intent: &SandboxIntent) -> String {
    match intent {
        SandboxIntent::RunPaletteCommand { command } => format!("palette:{command}"),
        SandboxIntent::ReadFile { path } => format!("read:{path}"),
        SandboxIntent::WriteFile { path } => format!("write:{path}"),
        SandboxIntent::SpawnProcess { program, args } => {
            if args.is_empty() {
                format!("spawn:{program}")
            } else {
                format!("spawn:{program} {}", args.join(" "))
            }
        }
    }
}

fn is_loop_control_command(command: &str) -> bool {
    command.starts_with("loop stop")
        || command.starts_with("loop kill")
        || command.starts_with("loop delete")
        || command.starts_with("loop resume")
        || command.starts_with("loop new")
}

fn is_blocked_path(policy: &SandboxPolicy, path: &str) -> bool {
    policy
        .blocked_path_prefixes
        .iter()
        .map(|prefix| normalize_scope(prefix))
        .any(|prefix| !prefix.is_empty() && path.starts_with(&prefix))
}

fn matches_allowed_root(roots: &[String], path: &str) -> bool {
    if roots.is_empty() {
        return true;
    }
    roots
        .iter()
        .map(|root| normalize_scope(root))
        .any(|root| !root.is_empty() && path.starts_with(&root))
}

fn is_expired(expires_at_epoch_s: Option<i64>, now_epoch_s: i64) -> bool {
    let Some(expires) = expires_at_epoch_s else {
        return false;
    };
    now_epoch_s > expires
}

fn normalize_id(value: &str) -> String {
    let mut out = String::new();
    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if (ch == '-' || ch == '_' || ch.is_ascii_whitespace()) && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_owned()
}

fn normalize_scope(value: &str) -> String {
    value.trim().replace('\\', "/").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::{
        evaluate_sandbox_request, SandboxCapability, SandboxGrant, SandboxGrantRegistry,
        SandboxIntent, SandboxPolicy, SandboxRequest,
    };
    use crate::extension_actions::ExtensionPermission;

    fn request<'a>(
        extension_id: &'a str,
        permissions: &'a [ExtensionPermission],
        intent: SandboxIntent,
    ) -> SandboxRequest<'a> {
        SandboxRequest {
            extension_id,
            permissions,
            now_epoch_s: 100,
            intent,
        }
    }

    #[test]
    fn filesystem_read_requires_explicit_grant() {
        let policy = SandboxPolicy::default();
        let grants = SandboxGrantRegistry::new();
        let decision = evaluate_sandbox_request(
            &request(
                "ext-one",
                &[ExtensionPermission::ReadState],
                SandboxIntent::ReadFile {
                    path: "./logs/run.log".to_owned(),
                },
            ),
            &policy,
            &grants,
        );
        assert!(!decision.allowed);
        assert_eq!(decision.capability, Some(SandboxCapability::FilesystemRead));
    }

    #[test]
    fn filesystem_read_allowed_with_grant() {
        let policy = SandboxPolicy::default();
        let mut grants = SandboxGrantRegistry::new();
        grants.grant(SandboxGrant {
            extension_id: "ext-one".to_owned(),
            capability: SandboxCapability::FilesystemRead,
            scope: "./logs".to_owned(),
            granted_by: "ops".to_owned(),
            reason: "allow logs inspection".to_owned(),
            expires_at_epoch_s: Some(500),
        });
        let decision = evaluate_sandbox_request(
            &request(
                "ext-one",
                &[ExtensionPermission::ReadState],
                SandboxIntent::ReadFile {
                    path: "./logs/run.log".to_owned(),
                },
            ),
            &policy,
            &grants,
        );
        assert!(decision.allowed);
        assert_eq!(decision.matched_grant_scope.as_deref(), Some("./logs"));
    }

    #[test]
    fn filesystem_write_requires_permission_and_grant() {
        let policy = SandboxPolicy::default();
        let mut grants = SandboxGrantRegistry::new();
        grants.grant(SandboxGrant {
            extension_id: "ext-one".to_owned(),
            capability: SandboxCapability::FilesystemWrite,
            scope: "./tmp".to_owned(),
            granted_by: "ops".to_owned(),
            reason: "temporary files".to_owned(),
            expires_at_epoch_s: None,
        });

        let denied = evaluate_sandbox_request(
            &request(
                "ext-one",
                &[ExtensionPermission::ReadState],
                SandboxIntent::WriteFile {
                    path: "./tmp/out.txt".to_owned(),
                },
            ),
            &policy,
            &grants,
        );
        assert!(!denied.allowed);

        let allowed = evaluate_sandbox_request(
            &request(
                "ext-one",
                &[ExtensionPermission::WriteState],
                SandboxIntent::WriteFile {
                    path: "./tmp/out.txt".to_owned(),
                },
            ),
            &policy,
            &grants,
        );
        assert!(allowed.allowed);
    }

    #[test]
    fn blocked_paths_always_denied() {
        let policy = SandboxPolicy::default();
        let mut grants = SandboxGrantRegistry::new();
        grants.grant(SandboxGrant {
            extension_id: "ext-one".to_owned(),
            capability: SandboxCapability::FilesystemRead,
            scope: "/etc".to_owned(),
            granted_by: "ops".to_owned(),
            reason: "should not pass".to_owned(),
            expires_at_epoch_s: None,
        });

        let decision = evaluate_sandbox_request(
            &request(
                "ext-one",
                &[ExtensionPermission::ReadState],
                SandboxIntent::ReadFile {
                    path: "/etc/passwd".to_owned(),
                },
            ),
            &policy,
            &grants,
        );
        assert!(!decision.allowed);
    }

    #[test]
    fn process_spawn_requires_permission_and_grant() {
        let policy = SandboxPolicy::default();
        let mut grants = SandboxGrantRegistry::new();
        grants.grant(SandboxGrant {
            extension_id: "ext-one".to_owned(),
            capability: SandboxCapability::ProcessSpawn,
            scope: "forge".to_owned(),
            granted_by: "ops".to_owned(),
            reason: "allow forge subcommands".to_owned(),
            expires_at_epoch_s: Some(1000),
        });

        let denied = evaluate_sandbox_request(
            &request(
                "ext-one",
                &[ExtensionPermission::ReadState],
                SandboxIntent::SpawnProcess {
                    program: "forge".to_owned(),
                    args: vec!["ps".to_owned()],
                },
            ),
            &policy,
            &grants,
        );
        assert!(!denied.allowed);

        let allowed = evaluate_sandbox_request(
            &request(
                "ext-one",
                &[ExtensionPermission::ExecuteShell],
                SandboxIntent::SpawnProcess {
                    program: "forge".to_owned(),
                    args: vec!["ps".to_owned()],
                },
            ),
            &policy,
            &grants,
        );
        assert!(allowed.allowed);
    }

    #[test]
    fn expired_grants_do_not_authorize() {
        let policy = SandboxPolicy::default();
        let mut grants = SandboxGrantRegistry::new();
        grants.grant(SandboxGrant {
            extension_id: "ext-one".to_owned(),
            capability: SandboxCapability::ProcessSpawn,
            scope: "forge".to_owned(),
            granted_by: "ops".to_owned(),
            reason: "short-lived".to_owned(),
            expires_at_epoch_s: Some(50),
        });
        let request = SandboxRequest {
            extension_id: "ext-one",
            permissions: &[ExtensionPermission::ExecuteShell],
            now_epoch_s: 100,
            intent: SandboxIntent::SpawnProcess {
                program: "forge".to_owned(),
                args: vec!["ps".to_owned()],
            },
        };
        let decision = evaluate_sandbox_request(&request, &policy, &grants);
        assert!(!decision.allowed);
    }

    #[test]
    fn exec_palette_command_uses_process_policy() {
        let policy = SandboxPolicy::default();
        let mut grants = SandboxGrantRegistry::new();
        grants.grant(SandboxGrant {
            extension_id: "ext-one".to_owned(),
            capability: SandboxCapability::ProcessSpawn,
            scope: "forge".to_owned(),
            granted_by: "ops".to_owned(),
            reason: "exec allowance".to_owned(),
            expires_at_epoch_s: None,
        });
        let denied = evaluate_sandbox_request(
            &request(
                "ext-one",
                &[ExtensionPermission::ReadState],
                SandboxIntent::RunPaletteCommand {
                    command: "exec forge ps".to_owned(),
                },
            ),
            &policy,
            &grants,
        );
        assert!(!denied.allowed);

        let allowed = evaluate_sandbox_request(
            &request(
                "ext-one",
                &[ExtensionPermission::ExecuteShell],
                SandboxIntent::RunPaletteCommand {
                    command: "exec forge ps".to_owned(),
                },
            ),
            &policy,
            &grants,
        );
        assert!(allowed.allowed);
    }

    #[test]
    fn non_sensitive_palette_command_allowed_without_grants() {
        let policy = SandboxPolicy::default();
        let grants = SandboxGrantRegistry::new();
        let decision = evaluate_sandbox_request(
            &request(
                "ext-one",
                &[ExtensionPermission::ReadState],
                SandboxIntent::RunPaletteCommand {
                    command: "view analytics".to_owned(),
                },
            ),
            &policy,
            &grants,
        );
        assert!(decision.allowed);
    }
}
