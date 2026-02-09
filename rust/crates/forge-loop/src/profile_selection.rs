use std::collections::BTreeMap;

pub const ERR_PROFILE_UNAVAILABLE: &str = "profile unavailable";
pub const ERR_POOL_UNAVAILABLE: &str = "pool unavailable";
pub const DEFAULT_WAIT_INTERVAL_SECONDS: i64 = 5;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoopSpec {
    pub profile_id: String,
    pub pool_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub max_concurrency: i32,
    pub cooldown_until_epoch: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MetaValue {
    Int(i64),
    Float(f64),
    Text(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pool {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub metadata: BTreeMap<String, MetaValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolMember {
    pub profile_id: String,
    pub position: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionResult {
    pub selected_profile: Option<Profile>,
    pub wait_until_epoch: Option<i64>,
}

pub trait SelectionBackend {
    fn get_profile(&self, profile_id: &str) -> Result<Profile, String>;
    fn get_pool(&self, pool_id: &str) -> Result<Pool, String>;
    fn get_pool_by_name(&self, name: &str) -> Result<Pool, String>;
    fn get_default_pool(&self) -> Result<Pool, String>;
    fn list_pool_members(&self, pool_id: &str) -> Result<Vec<PoolMember>, String>;
    fn count_running_by_profile(&self, profile_id: &str) -> Result<i32, String>;
    fn update_pool(&mut self, pool: &Pool) -> Result<(), String>;
}

#[derive(Debug, Clone, Default)]
pub struct InMemorySelectionBackend {
    profiles_by_id: BTreeMap<String, Profile>,
    pools_by_id: BTreeMap<String, Pool>,
    pools_by_name: BTreeMap<String, String>,
    members_by_pool: BTreeMap<String, Vec<PoolMember>>,
    running_by_profile: BTreeMap<String, i32>,
}

impl InMemorySelectionBackend {
    pub fn with_profiles(mut self, profiles: Vec<Profile>) -> Self {
        for profile in profiles {
            self.profiles_by_id.insert(profile.id.clone(), profile);
        }
        self
    }

    pub fn with_pools(mut self, pools: Vec<Pool>) -> Self {
        for pool in pools {
            self.pools_by_name
                .insert(pool.name.clone(), pool.id.clone());
            self.pools_by_id.insert(pool.id.clone(), pool);
        }
        self
    }

    pub fn with_pool_members(mut self, pool_id: &str, members: Vec<PoolMember>) -> Self {
        self.members_by_pool.insert(pool_id.to_string(), members);
        self
    }

    pub fn with_running_count(mut self, profile_id: &str, count: i32) -> Self {
        self.running_by_profile
            .insert(profile_id.to_string(), count);
        self
    }
}

impl SelectionBackend for InMemorySelectionBackend {
    fn get_profile(&self, profile_id: &str) -> Result<Profile, String> {
        self.profiles_by_id
            .get(profile_id)
            .cloned()
            .ok_or_else(|| format!("profile not found: {profile_id}"))
    }

    fn get_pool(&self, pool_id: &str) -> Result<Pool, String> {
        self.pools_by_id
            .get(pool_id)
            .cloned()
            .ok_or_else(|| format!("pool not found: {pool_id}"))
    }

    fn get_pool_by_name(&self, name: &str) -> Result<Pool, String> {
        let id = self
            .pools_by_name
            .get(name)
            .ok_or_else(|| format!("pool not found by name: {name}"))?;
        self.get_pool(id)
    }

    fn get_default_pool(&self) -> Result<Pool, String> {
        self.pools_by_id
            .values()
            .find(|pool| pool.is_default)
            .cloned()
            .ok_or_else(|| "default pool not found".to_string())
    }

    fn list_pool_members(&self, pool_id: &str) -> Result<Vec<PoolMember>, String> {
        Ok(self
            .members_by_pool
            .get(pool_id)
            .cloned()
            .unwrap_or_default())
    }

    fn count_running_by_profile(&self, profile_id: &str) -> Result<i32, String> {
        Ok(*self.running_by_profile.get(profile_id).unwrap_or(&0))
    }

    fn update_pool(&mut self, pool: &Pool) -> Result<(), String> {
        self.pools_by_name
            .insert(pool.name.clone(), pool.id.clone());
        self.pools_by_id.insert(pool.id.clone(), pool.clone());
        Ok(())
    }
}

pub fn select_profile(
    backend: &mut dyn SelectionBackend,
    loop_spec: &LoopSpec,
    default_pool_name: &str,
    now_epoch: i64,
) -> Result<SelectionResult, String> {
    if !loop_spec.profile_id.is_empty() {
        let profile = backend.get_profile(&loop_spec.profile_id)?;
        let (available, _, _) = profile_available(backend, &profile, now_epoch)?;
        if !available {
            return Err(format!("pinned profile {} unavailable", profile.name));
        }
        return Ok(SelectionResult {
            selected_profile: Some(profile),
            wait_until_epoch: None,
        });
    }

    let mut pool = resolve_pool(backend, loop_spec, default_pool_name)?;
    let members = backend.list_pool_members(&pool.id)?;
    if members.is_empty() {
        return Err(ERR_POOL_UNAVAILABLE.to_string());
    }

    let start_index = pool_last_index(&pool);
    let mut earliest_wait: Option<i64> = None;

    for i in 0..members.len() {
        let idx = (start_index + 1 + i as i32).rem_euclid(members.len() as i32);
        let member = &members[idx as usize];
        let profile = match backend.get_profile(&member.profile_id) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let (available, next_wait, _) = match profile_available(backend, &profile, now_epoch) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if available {
            set_pool_last_index(&mut pool, idx);
            let _ = backend.update_pool(&pool);
            return Ok(SelectionResult {
                selected_profile: Some(profile),
                wait_until_epoch: None,
            });
        }
        if let Some(next) = next_wait {
            earliest_wait = Some(match earliest_wait {
                Some(existing) if existing <= next => existing,
                _ => next,
            });
        }
    }

    let wait_until = earliest_wait.unwrap_or(now_epoch + DEFAULT_WAIT_INTERVAL_SECONDS);
    Ok(SelectionResult {
        selected_profile: None,
        wait_until_epoch: Some(wait_until),
    })
}

fn profile_available(
    backend: &dyn SelectionBackend,
    profile: &Profile,
    now_epoch: i64,
) -> Result<(bool, Option<i64>, Option<String>), String> {
    if let Some(cooldown_until) = profile.cooldown_until_epoch {
        if cooldown_until > now_epoch {
            return Ok((false, Some(cooldown_until), None));
        }
    }

    if profile.max_concurrency > 0 {
        let count = backend.count_running_by_profile(&profile.id)?;
        if count >= profile.max_concurrency {
            return Ok((false, None, None));
        }
    }

    Ok((true, None, None))
}

fn resolve_pool(
    backend: &dyn SelectionBackend,
    loop_spec: &LoopSpec,
    default_pool_name: &str,
) -> Result<Pool, String> {
    if !loop_spec.pool_id.is_empty() {
        return backend.get_pool(&loop_spec.pool_id);
    }
    if !default_pool_name.is_empty() {
        if let Ok(pool) = backend.get_pool_by_name(default_pool_name) {
            return Ok(pool);
        }
    }
    backend
        .get_default_pool()
        .map_err(|_| ERR_POOL_UNAVAILABLE.to_string())
}

fn pool_last_index(pool: &Pool) -> i32 {
    let Some(value) = pool.metadata.get("last_index") else {
        return -1;
    };
    match value {
        MetaValue::Int(v) => *v as i32,
        MetaValue::Float(v) => *v as i32,
        MetaValue::Text(v) => v.parse::<i32>().unwrap_or(-1),
    }
}

fn set_pool_last_index(pool: &mut Pool, idx: i32) {
    pool.metadata
        .insert("last_index".to_string(), MetaValue::Int(idx as i64));
}

#[cfg(test)]
mod tests {
    use super::{
        select_profile, InMemorySelectionBackend, LoopSpec, MetaValue, Pool, PoolMember, Profile,
        SelectionBackend, DEFAULT_WAIT_INTERVAL_SECONDS, ERR_POOL_UNAVAILABLE,
    };
    use std::collections::BTreeMap;

    #[test]
    fn pinned_profile_unavailable_when_max_concurrency_reached() {
        let now = 1_700_000_000i64;
        let mut backend = InMemorySelectionBackend::default()
            .with_profiles(vec![Profile {
                id: "profile-1".to_string(),
                name: "p1".to_string(),
                max_concurrency: 1,
                cooldown_until_epoch: None,
            }])
            .with_running_count("profile-1", 1);

        let err = match select_profile(
            &mut backend,
            &LoopSpec {
                profile_id: "profile-1".to_string(),
                pool_id: String::new(),
            },
            "",
            now,
        ) {
            Ok(_) => panic!("expected unavailable pinned profile error"),
            Err(err) => err,
        };
        assert_eq!(err, "pinned profile p1 unavailable");
    }

    #[test]
    fn pool_selection_skips_cooldown_profile() {
        let now = 1_700_000_000i64;
        let pool = Pool {
            id: "pool-a".to_string(),
            name: "pool-a".to_string(),
            is_default: true,
            metadata: BTreeMap::new(),
        };
        let mut backend = InMemorySelectionBackend::default()
            .with_profiles(vec![
                Profile {
                    id: "profile-cool".to_string(),
                    name: "cooldown".to_string(),
                    max_concurrency: 0,
                    cooldown_until_epoch: Some(now + 600),
                },
                Profile {
                    id: "profile-ready".to_string(),
                    name: "ready".to_string(),
                    max_concurrency: 0,
                    cooldown_until_epoch: None,
                },
            ])
            .with_pools(vec![pool.clone()])
            .with_pool_members(
                &pool.id,
                vec![
                    PoolMember {
                        profile_id: "profile-cool".to_string(),
                        position: 1,
                    },
                    PoolMember {
                        profile_id: "profile-ready".to_string(),
                        position: 2,
                    },
                ],
            );

        let result = match select_profile(&mut backend, &LoopSpec::default(), "", now) {
            Ok(value) => value,
            Err(err) => panic!("unexpected error: {err}"),
        };
        let selected = match result.selected_profile {
            Some(value) => value,
            None => panic!("expected selected profile"),
        };
        assert_eq!(selected.id, "profile-ready");
        assert_eq!(result.wait_until_epoch, None);
    }

    #[test]
    fn pool_selection_waits_for_earliest_cooldown() {
        let now = 1_700_000_000i64;
        let early = now + 300;
        let late = now + 600;
        let pool = Pool {
            id: "pool-b".to_string(),
            name: "pool-b".to_string(),
            is_default: true,
            metadata: BTreeMap::new(),
        };
        let mut backend = InMemorySelectionBackend::default()
            .with_profiles(vec![
                Profile {
                    id: "profile-early".to_string(),
                    name: "cooldown-early".to_string(),
                    max_concurrency: 0,
                    cooldown_until_epoch: Some(early),
                },
                Profile {
                    id: "profile-late".to_string(),
                    name: "cooldown-late".to_string(),
                    max_concurrency: 0,
                    cooldown_until_epoch: Some(late),
                },
            ])
            .with_pools(vec![pool.clone()])
            .with_pool_members(
                &pool.id,
                vec![
                    PoolMember {
                        profile_id: "profile-early".to_string(),
                        position: 1,
                    },
                    PoolMember {
                        profile_id: "profile-late".to_string(),
                        position: 2,
                    },
                ],
            );

        let result = match select_profile(&mut backend, &LoopSpec::default(), "", now) {
            Ok(value) => value,
            Err(err) => panic!("unexpected error: {err}"),
        };
        assert!(result.selected_profile.is_none());
        assert_eq!(result.wait_until_epoch, Some(early));
    }

    #[test]
    fn all_profiles_busy_without_cooldown_uses_default_wait_interval() {
        let now = 1_700_000_000i64;
        let pool = Pool {
            id: "pool-c".to_string(),
            name: "pool-c".to_string(),
            is_default: true,
            metadata: BTreeMap::new(),
        };
        let mut backend = InMemorySelectionBackend::default()
            .with_profiles(vec![Profile {
                id: "profile-busy".to_string(),
                name: "busy".to_string(),
                max_concurrency: 1,
                cooldown_until_epoch: None,
            }])
            .with_pools(vec![pool.clone()])
            .with_pool_members(
                &pool.id,
                vec![PoolMember {
                    profile_id: "profile-busy".to_string(),
                    position: 1,
                }],
            )
            .with_running_count("profile-busy", 1);

        let result = match select_profile(&mut backend, &LoopSpec::default(), "", now) {
            Ok(value) => value,
            Err(err) => panic!("unexpected error: {err}"),
        };
        assert!(result.selected_profile.is_none());
        assert_eq!(
            result.wait_until_epoch,
            Some(now + DEFAULT_WAIT_INTERVAL_SECONDS)
        );
    }

    #[test]
    fn resolve_pool_prefers_loop_pool_then_named_default_then_default_flag() {
        let mut meta = BTreeMap::new();
        meta.insert("last_index".to_string(), MetaValue::Text("0".to_string()));
        let loop_pool = Pool {
            id: "pool-loop".to_string(),
            name: "loop-specific".to_string(),
            is_default: false,
            metadata: meta,
        };
        let named_default = Pool {
            id: "pool-named".to_string(),
            name: "named-default".to_string(),
            is_default: false,
            metadata: BTreeMap::new(),
        };
        let fallback_default = Pool {
            id: "pool-default".to_string(),
            name: "fallback".to_string(),
            is_default: true,
            metadata: BTreeMap::new(),
        };
        let profile = Profile {
            id: "profile-1".to_string(),
            name: "p1".to_string(),
            max_concurrency: 0,
            cooldown_until_epoch: None,
        };

        let mut backend = InMemorySelectionBackend::default()
            .with_profiles(vec![profile.clone()])
            .with_pools(vec![
                loop_pool.clone(),
                named_default.clone(),
                fallback_default.clone(),
            ])
            .with_pool_members(
                &loop_pool.id,
                vec![PoolMember {
                    profile_id: profile.id.clone(),
                    position: 1,
                }],
            )
            .with_pool_members(
                &named_default.id,
                vec![PoolMember {
                    profile_id: profile.id.clone(),
                    position: 1,
                }],
            )
            .with_pool_members(
                &fallback_default.id,
                vec![PoolMember {
                    profile_id: profile.id.clone(),
                    position: 1,
                }],
            );

        let loop_result = match select_profile(
            &mut backend,
            &LoopSpec {
                profile_id: String::new(),
                pool_id: "pool-loop".to_string(),
            },
            "named-default",
            1_700_000_000,
        ) {
            Ok(value) => value,
            Err(err) => panic!("unexpected error: {err}"),
        };
        assert_eq!(
            loop_result
                .selected_profile
                .as_ref()
                .map(|profile| profile.id.as_str()),
            Some("profile-1")
        );

        let named_result = match select_profile(
            &mut backend,
            &LoopSpec::default(),
            "named-default",
            1_700_000_000,
        ) {
            Ok(value) => value,
            Err(err) => panic!("unexpected error: {err}"),
        };
        assert_eq!(
            named_result
                .selected_profile
                .as_ref()
                .map(|profile| profile.id.as_str()),
            Some("profile-1")
        );
    }

    #[test]
    fn round_robin_uses_and_updates_last_index_metadata() {
        let mut metadata = BTreeMap::new();
        metadata.insert("last_index".to_string(), MetaValue::Text("0".to_string()));
        let pool = Pool {
            id: "pool-rr".to_string(),
            name: "rr".to_string(),
            is_default: true,
            metadata,
        };

        let profile_a = Profile {
            id: "profile-a".to_string(),
            name: "a".to_string(),
            max_concurrency: 0,
            cooldown_until_epoch: None,
        };
        let profile_b = Profile {
            id: "profile-b".to_string(),
            name: "b".to_string(),
            max_concurrency: 0,
            cooldown_until_epoch: None,
        };
        let mut backend = InMemorySelectionBackend::default()
            .with_profiles(vec![profile_a.clone(), profile_b.clone()])
            .with_pools(vec![pool.clone()])
            .with_pool_members(
                &pool.id,
                vec![
                    PoolMember {
                        profile_id: profile_a.id.clone(),
                        position: 1,
                    },
                    PoolMember {
                        profile_id: profile_b.id.clone(),
                        position: 2,
                    },
                ],
            );

        let first = match select_profile(&mut backend, &LoopSpec::default(), "", 1_700_000_000) {
            Ok(value) => value,
            Err(err) => panic!("unexpected error: {err}"),
        };
        assert_eq!(
            first
                .selected_profile
                .as_ref()
                .map(|profile| profile.id.as_str()),
            Some("profile-b")
        );

        let updated_pool = match backend.get_pool("pool-rr") {
            Ok(value) => value,
            Err(err) => panic!("expected updated pool: {err}"),
        };
        assert_eq!(
            updated_pool.metadata.get("last_index"),
            Some(&MetaValue::Int(1))
        );
    }

    #[test]
    fn pool_unavailable_when_default_missing() {
        let mut backend = InMemorySelectionBackend::default();
        let err = match select_profile(&mut backend, &LoopSpec::default(), "", 1_700_000_000) {
            Ok(_) => panic!("expected pool unavailable"),
            Err(err) => err,
        };
        assert_eq!(err, ERR_POOL_UNAVAILABLE);
    }
}
