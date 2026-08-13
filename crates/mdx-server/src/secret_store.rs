// Tenant-scoped model-gateway credentials and provider metadata.
//
// A key pasted in the GUI is held in memory and, when the operator asks, in the
// macOS Keychain through the native Security framework. It is never written to
// receipts or app-state exports. Environment credentials remain an explicit
// deployment option. "Model connected" is only honest when one of these stores
// can supply a usable credential now.
//
// Credential references are resolved exactly. A configured connection never
// silently borrows a different provider key. Supported backends cover session
// memory, deployment environment, macOS Keychain, tenant-isolated mounted
// secret files, and an HTTPS or loopback secret broker. The mounted and broker
// forms let Docker, Kubernetes, Vault, AWS, GCP, and Azure adapters stay outside
// the kernel while Model Fabric keeps one connection contract.
use std::collections::HashMap;
#[cfg(all(not(test), target_os = "macos"))]
use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock};
use std::{fs, io::Read, path::Path};

#[cfg(all(not(test), target_os = "macos"))]
const KEYCHAIN_SERVICE_PREFIX: &str = "mdx-native-provider-key";
const MAX_SECRET_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CredentialResolution {
    pub value: String,
    pub source: &'static str,
}

#[derive(Default)]
pub(crate) struct SecretStore {
    keys: Mutex<HashMap<String, String>>,
    models: Mutex<HashMap<String, Vec<String>>>,
    role_slots: Mutex<HashMap<String, String>>,
    keychain_cache: Arc<Mutex<HashMap<String, Option<String>>>>,
    #[cfg(all(not(test), target_os = "macos"))]
    keychain_loading: Arc<Mutex<HashSet<String>>>,
}

fn scoped_key(tenant_id: &str, env_key: &str) -> String {
    format!("{}\u{1f}{}", tenant_id.trim(), env_key.trim())
}

fn scoped_role_key(tenant_id: &str, role: &str) -> String {
    format!(
        "{}\u{1f}{}",
        tenant_id.trim(),
        role.trim().to_ascii_lowercase()
    )
}

impl SecretStore {
    /// Hold a provider key in memory for one tenant. Environment credentials
    /// remain explicit process-wide configuration; GUI-pasted keys do not cross
    /// tenant boundaries inside the same process.
    pub(crate) fn set_for_tenant(&self, tenant_id: &str, env_key: &str, value: &str) {
        if let Ok(mut keys) = self.keys.lock() {
            keys.insert(scoped_key(tenant_id, env_key), value.to_string());
        }
    }

    pub(crate) fn remove_for_tenant(&self, tenant_id: &str, env_key: &str) -> bool {
        let scoped = scoped_key(tenant_id, env_key);
        let removed_key = self
            .keys
            .lock()
            .ok()
            .and_then(|mut keys| keys.remove(&scoped))
            .is_some();
        let removed_models = self
            .models
            .lock()
            .ok()
            .and_then(|mut models| models.remove(&scoped))
            .is_some();
        removed_key || removed_models
    }

    /// Forget credential material owned by this process or this Mac. Secret
    /// references owned by an environment, mounted volume, broker, or managed
    /// service are deliberately not deleted here. Model Fabric still revokes
    /// their connection immediately, while the operator retains secret
    /// lifecycle authority in the external system.
    pub(crate) fn revoke_local_credential_for_connection(
        &self,
        tenant_id: &str,
        credential_ref: &str,
        fallback_env: &str,
    ) -> Result<bool, String> {
        let credential_ref = credential_ref.trim();
        let session_name = credential_ref
            .strip_prefix("secret:session/")
            .or_else(|| credential_ref.is_empty().then_some(fallback_env));
        if let Some(name) = session_name {
            return Ok(self.remove_for_tenant(tenant_id, name));
        }
        let keychain_name = credential_ref
            .strip_prefix("secret:local_keychain/")
            .or_else(|| credential_ref.strip_prefix("secret:keychain/"))
            .or_else(|| credential_ref.strip_prefix("keychain:"));
        let Some(name) = keychain_name else {
            return Ok(false);
        };
        self.remove_for_tenant(tenant_id, name);
        let scoped = scoped_key(tenant_id, name);
        if let Ok(mut cache) = self.keychain_cache.lock() {
            cache.insert(scoped, None);
        }
        #[cfg(all(not(test), target_os = "macos"))]
        {
            security_framework::passwords::delete_generic_password(
                &keychain_service(tenant_id, name),
                &keychain_account(tenant_id, name),
            )
            .map_err(|error| {
                format!("macOS Keychain could not forget the provider key ({error})")
            })?;
            Ok(true)
        }
        #[cfg(any(test, not(target_os = "macos")))]
        {
            Ok(true)
        }
    }

    pub(crate) fn provider_key_for_tenant(&self, tenant_id: &str, env_key: &str) -> Option<String> {
        if let Ok(keys) = self.keys.lock()
            && let Some(value) = keys
                .get(&scoped_key(tenant_id, env_key))
                .filter(|value| !value.is_empty())
        {
            return Some(value.clone());
        }
        if let Some(value) = std::env::var(env_key)
            .ok()
            .filter(|value| !value.is_empty())
        {
            return Some(value);
        }
        self.keychain_key_for_tenant(tenant_id, env_key)
    }

    /// Resolve the credential reference attached to a Model Fabric connection.
    /// The fallback environment name is used only for old receipts that did not
    /// yet carry a canonical reference.
    pub(crate) fn credential_for_connection(
        &self,
        tenant_id: &str,
        credential_ref: &str,
        fallback_env: &str,
    ) -> Option<CredentialResolution> {
        let credential_ref = credential_ref.trim();
        if credential_ref.is_empty() {
            return self
                .provider_key_for_tenant(tenant_id, fallback_env)
                .map(|value| CredentialResolution {
                    value,
                    source: self.provider_key_source_for_tenant(tenant_id, fallback_env),
                });
        }
        if let Some(name) = credential_ref.strip_prefix("secret:session/") {
            return self.in_memory_key_for_tenant(tenant_id, name).map(|value| {
                CredentialResolution {
                    value,
                    source: "session",
                }
            });
        }
        if let Some(name) = credential_ref.strip_prefix("secret:local_keychain/") {
            return self.keychain_key_for_tenant(tenant_id, name).map(|value| {
                CredentialResolution {
                    value,
                    source: "local_keychain",
                }
            });
        }
        if let Some(name) = credential_ref
            .strip_prefix("keychain:")
            .or_else(|| credential_ref.strip_prefix("secret:keychain/"))
        {
            return self.keychain_key_for_tenant(tenant_id, name).map(|value| {
                CredentialResolution {
                    value,
                    source: "local_keychain",
                }
            });
        }
        if let Some(name) = environment_reference_name(credential_ref) {
            return environment_credential(name).map(|value| CredentialResolution {
                value,
                source: "env",
            });
        }
        if let Some(name) = credential_ref
            .strip_prefix("secret:mounted/")
            .or_else(|| credential_ref.strip_prefix("file:"))
        {
            return mounted_root("MDX_MODEL_SECRET_DIR")
                .and_then(|root| read_mounted_credential(&root, tenant_id, name).ok())
                .map(|value| CredentialResolution {
                    value,
                    source: "mounted",
                });
        }
        if let Some(name) = credential_ref.strip_prefix("secret:managed/") {
            return mounted_root("MDX_MANAGED_MODEL_SECRET_DIR")
                .and_then(|root| read_managed_credential(&root, name).ok())
                .map(|value| CredentialResolution {
                    value,
                    source: "managed",
                });
        }
        if credential_ref.starts_with("secret:broker/")
            || credential_ref.starts_with("broker:")
            || credential_ref.starts_with("vault:")
        {
            return broker_credential(tenant_id, credential_ref).map(|value| {
                CredentialResolution {
                    value,
                    source: "broker",
                }
            });
        }
        None
    }

    pub(crate) fn has_credential_for_connection(
        &self,
        tenant_id: &str,
        credential_ref: &str,
        fallback_env: &str,
    ) -> bool {
        self.credential_for_connection(tenant_id, credential_ref, fallback_env)
            .is_some_and(|credential| !credential.value.trim().is_empty())
    }

    pub(crate) fn request_credential_refresh_for_connection(
        &self,
        tenant_id: &str,
        credential_ref: &str,
        fallback_env: &str,
    ) {
        let keychain_name = credential_ref
            .strip_prefix("secret:local_keychain/")
            .or_else(|| credential_ref.strip_prefix("secret:keychain/"))
            .or_else(|| credential_ref.strip_prefix("keychain:"));
        if let Some(name) = keychain_name {
            self.request_keychain_refresh_for_tenant(tenant_id, name);
        } else if credential_ref.trim().is_empty() {
            self.request_keychain_refresh_for_tenant(tenant_id, fallback_env);
        }
    }

    /// Resolve an explicitly selected Keychain credential during a human
    /// operator action. Unlike passive readiness reads, this path may ask
    /// macOS to authorize access. Successful material is cached in memory so
    /// subsequent routing stays prompt-free and no secret enters a receipt.
    pub(crate) fn authorize_keychain_credential_for_connection(
        &self,
        tenant_id: &str,
        credential_ref: &str,
        fallback_env: &str,
    ) -> Option<CredentialResolution> {
        let credential_ref = credential_ref.trim();
        let keychain_name = credential_ref
            .strip_prefix("secret:local_keychain/")
            .or_else(|| credential_ref.strip_prefix("secret:keychain/"))
            .or_else(|| credential_ref.strip_prefix("keychain:"))
            .or_else(|| credential_ref.is_empty().then_some(fallback_env))?;
        #[cfg(all(not(test), target_os = "macos"))]
        {
            let scoped = scoped_key(tenant_id, keychain_name);
            let should_start = self
                .keychain_loading
                .lock()
                .ok()
                .is_some_and(|mut loading| loading.insert(scoped.clone()));
            if !should_start {
                for _ in 0..100 {
                    if let Some(value) = self.keychain_key_for_tenant(tenant_id, keychain_name) {
                        return Some(CredentialResolution {
                            value,
                            source: "local_keychain",
                        });
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                return None;
            }
            let cache = Arc::clone(&self.keychain_cache);
            let loading = Arc::clone(&self.keychain_loading);
            let service = keychain_service(tenant_id, keychain_name);
            let account = keychain_account(tenant_id, keychain_name);
            let (sender, receiver) = std::sync::mpsc::sync_channel(1);
            let spawned = std::thread::Builder::new()
                .name("mdx-keychain-authorize".to_string())
                .spawn(move || {
                    let value =
                        security_framework::passwords::get_generic_password(&service, &account)
                            .ok()
                            .and_then(|value| String::from_utf8(value).ok())
                            .map(|value| value.trim().to_string())
                            .filter(|value| !value.is_empty());
                    if let Ok(mut cache) = cache.lock() {
                        cache.insert(scoped.clone(), value.clone());
                    }
                    if let Ok(mut loading) = loading.lock() {
                        loading.remove(&scoped);
                    }
                    let _ = sender.send(value);
                });
            if spawned.is_err() {
                if let Ok(mut loading) = self.keychain_loading.lock() {
                    loading.remove(&scoped_key(tenant_id, keychain_name));
                }
                return None;
            }
            receiver
                .recv_timeout(std::time::Duration::from_secs(5))
                .ok()
                .flatten()
                .map(|value| CredentialResolution {
                    value,
                    source: "local_keychain",
                })
        }
        #[cfg(any(test, not(target_os = "macos")))]
        {
            let _ = (tenant_id, keychain_name);
            None
        }
    }

    #[cfg(test)]
    pub(crate) fn disable_keychain_lookup_for_tenant(&self, tenant_id: &str, env_key: &str) {
        if let Ok(mut cache) = self.keychain_cache.lock() {
            cache.insert(scoped_key(tenant_id, env_key), None);
        }
    }

    pub(crate) fn has_provider_key_for_tenant(&self, tenant_id: &str, env_key: &str) -> bool {
        self.provider_key_for_tenant(tenant_id, env_key).is_some()
    }

    pub(crate) fn provider_key_source_for_tenant(
        &self,
        tenant_id: &str,
        env_key: &str,
    ) -> &'static str {
        if self.in_memory_key_for_tenant(tenant_id, env_key).is_some() {
            "session"
        } else if std::env::var(env_key)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .is_some()
        {
            "env"
        } else if self.keychain_key_for_tenant(tenant_id, env_key).is_some() {
            "local_keychain"
        } else {
            "none"
        }
    }

    pub(crate) fn in_memory_key_for_tenant(
        &self,
        tenant_id: &str,
        env_key: &str,
    ) -> Option<String> {
        self.keys.lock().ok().and_then(|keys| {
            keys.get(&scoped_key(tenant_id, env_key))
                .filter(|v| !v.is_empty())
                .cloned()
        })
    }

    pub(crate) fn set_models_for_tenant(&self, tenant_id: &str, env_key: &str, models: &[String]) {
        if let Ok(mut catalog) = self.models.lock() {
            catalog.insert(scoped_key(tenant_id, env_key), models.to_vec());
        }
    }

    pub(crate) fn persist_for_tenant(
        &self,
        tenant_id: &str,
        env_key: &str,
        value: &str,
    ) -> Result<&'static str, String> {
        if value.trim().is_empty() {
            return Err("provider key is empty".to_string());
        }
        #[cfg(test)]
        {
            let _ = (tenant_id, env_key, value);
            Err("Keychain persistence is disabled in tests".to_string())
        }
        #[cfg(all(not(test), target_os = "macos"))]
        {
            let scoped = scoped_key(tenant_id, env_key);
            security_framework::passwords::set_generic_password(
                &keychain_service(tenant_id, env_key),
                &keychain_account(tenant_id, env_key),
                value.as_bytes(),
            )
            .map_err(|error| format!("macOS Keychain refused the provider key ({error})"))?;
            if let Ok(mut cache) = self.keychain_cache.lock() {
                cache.insert(scoped, Some(value.to_string()));
            }
            Ok("local_keychain")
        }
        #[cfg(all(not(test), not(target_os = "macos")))]
        {
            let root = mounted_root("MDX_MODEL_SECRET_DIR").ok_or_else(|| {
                "set MDX_MODEL_SECRET_DIR to enable durable local credential storage on this platform"
                    .to_string()
            })?;
            persist_mounted_credential(&root, tenant_id, env_key, value)?;
            Ok("mounted")
        }
    }

    fn keychain_key_for_tenant(&self, tenant_id: &str, env_key: &str) -> Option<String> {
        let scoped = scoped_key(tenant_id, env_key);
        self.keychain_cache
            .lock()
            .ok()
            .and_then(|cache| cache.get(&scoped).cloned())
            .flatten()
    }

    /// Ask the OS secret store to hydrate one explicitly configured key in
    /// the background. Read paths remain cache-only and therefore never block
    /// or open authentication UI. Until hydration finishes, routing fails
    /// closed and status surfaces report the credential as unavailable.
    pub(crate) fn request_keychain_refresh_for_tenant(&self, tenant_id: &str, env_key: &str) {
        let scoped = scoped_key(tenant_id, env_key);
        if self
            .keychain_cache
            .lock()
            .ok()
            .is_some_and(|cache| cache.contains_key(&scoped))
        {
            return;
        }
        #[cfg(all(not(test), target_os = "macos"))]
        {
            let should_start = self
                .keychain_loading
                .lock()
                .ok()
                .is_some_and(|mut loading| loading.insert(scoped.clone()));
            if !should_start {
                return;
            }
            let cache = Arc::clone(&self.keychain_cache);
            let loading = Arc::clone(&self.keychain_loading);
            let service = keychain_service(tenant_id, env_key);
            let account = keychain_account(tenant_id, env_key);
            std::thread::Builder::new()
                .name("mdx-keychain-hydrate".to_string())
                .spawn(move || {
                    let mut search = security_framework::item::ItemSearchOptions::new();
                    search
                        .class(security_framework::item::ItemClass::generic_password())
                        .service(&service)
                        .account(&account)
                        .load_data(true)
                        .skip_authenticated_items(true);
                    let value = search
                        .search()
                        .ok()
                        .and_then(|mut results| results.pop())
                        .and_then(|result| match result {
                            security_framework::item::SearchResult::Data(value) => Some(value),
                            _ => None,
                        })
                        .and_then(|value| String::from_utf8(value).ok())
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty());
                    if let Ok(mut cache) = cache.lock() {
                        cache.insert(scoped.clone(), value);
                    }
                    if let Ok(mut loading) = loading.lock() {
                        loading.remove(&scoped);
                    }
                })
                .ok();
        }
        #[cfg(any(test, not(target_os = "macos")))]
        {
            let _ = (scoped, tenant_id, env_key);
        }
    }

    pub(crate) fn models_for_tenant(&self, tenant_id: &str, env_key: &str) -> Vec<String> {
        self.models
            .lock()
            .ok()
            .and_then(|catalog| catalog.get(&scoped_key(tenant_id, env_key)).cloned())
            .unwrap_or_default()
    }

    pub(crate) fn set_role_slot_for_tenant(&self, tenant_id: &str, role: &str, slot: &str) {
        if let Ok(mut role_slots) = self.role_slots.lock() {
            let key = scoped_role_key(tenant_id, role);
            let slot = slot.trim().to_ascii_uppercase();
            if slot.is_empty() || slot == "AUTO" {
                role_slots.remove(&key);
            } else {
                role_slots.insert(key, slot);
            }
        }
    }

    pub(crate) fn role_slot_for_tenant(&self, tenant_id: &str, role: &str) -> Option<String> {
        self.role_slots.lock().ok().and_then(|role_slots| {
            role_slots
                .get(&scoped_role_key(tenant_id, role))
                .filter(|slot| !slot.trim().is_empty())
                .cloned()
        })
    }

    pub(crate) fn remove_role_slots_for_tenant_matching<F>(
        &self,
        tenant_id: &str,
        mut should_remove: F,
    ) -> usize
    where
        F: FnMut(&str) -> bool,
    {
        let prefix = format!("{}\u{1f}", tenant_id.trim());
        let mut removed = 0;
        if let Ok(mut role_slots) = self.role_slots.lock() {
            role_slots.retain(|key, slot| {
                let keep = !key.starts_with(&prefix) || !should_remove(slot);
                if !keep {
                    removed += 1;
                }
                keep
            });
        }
        removed
    }

    #[cfg(test)]
    pub(crate) fn clear_tenant(&self, tenant_id: &str) {
        let prefix = format!("{}\u{1f}", tenant_id.trim());
        if let Ok(mut keys) = self.keys.lock() {
            keys.retain(|key, _| !key.starts_with(&prefix));
        }
        if let Ok(mut models) = self.models.lock() {
            models.retain(|key, _| !key.starts_with(&prefix));
        }
        if let Ok(mut role_slots) = self.role_slots.lock() {
            role_slots.retain(|key, _| !key.starts_with(&prefix));
        }
    }
}

fn environment_credential(name: &str) -> Option<String> {
    valid_secret_name(name)
        .then(|| std::env::var(name).ok())
        .flatten()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn environment_reference_name(credential_ref: &str) -> Option<&str> {
    credential_ref
        .strip_prefix("secret:environment/")
        .or_else(|| credential_ref.strip_prefix("env:"))
        .filter(|name| valid_secret_name(name))
}

fn mounted_root(env_name: &str) -> Option<std::path::PathBuf> {
    std::env::var_os(env_name)
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
}

fn valid_secret_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn tenant_directory_name(tenant_id: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in tenant_id.trim().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    let readable = tenant_id
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .take(48)
        .collect::<String>();
    format!("{readable}-{hash:016x}")
}

fn read_mounted_credential(root: &Path, tenant_id: &str, name: &str) -> Result<String, String> {
    if !valid_secret_name(name) {
        return Err("mounted credential name is invalid".to_string());
    }
    read_secret_file(&root.join(tenant_directory_name(tenant_id)).join(name))
}

fn read_managed_credential(root: &Path, name: &str) -> Result<String, String> {
    if !valid_secret_name(name) {
        return Err("managed credential name is invalid".to_string());
    }
    read_secret_file(&root.join(name))
}

fn read_secret_file(path: &Path) -> Result<String, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("credential file is unavailable ({error})"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("credential path must be a regular non-symlink file".to_string());
    }
    if metadata.len() > MAX_SECRET_BYTES {
        return Err("credential file exceeds the 64 KiB limit".to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.mode() & 0o022 != 0 {
            return Err("credential file must not be group or world writable".to_string());
        }
    }
    let file = fs::File::open(path)
        .map_err(|error| format!("credential file could not be opened ({error})"))?;
    let mut value = String::new();
    file.take(MAX_SECRET_BYTES + 1)
        .read_to_string(&mut value)
        .map_err(|error| format!("credential file could not be read ({error})"))?;
    let value = value.trim().to_string();
    if value.is_empty() {
        Err("credential file is empty".to_string())
    } else {
        Ok(value)
    }
}

#[cfg(all(not(test), not(target_os = "macos")))]
fn persist_mounted_credential(
    root: &Path,
    tenant_id: &str,
    name: &str,
    value: &str,
) -> Result<(), String> {
    use std::io::Write;
    #[cfg(unix)]
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

    if !valid_secret_name(name) {
        return Err("mounted credential name is invalid".to_string());
    }
    let tenant_directory = root.join(tenant_directory_name(tenant_id));
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    builder.mode(0o700);
    builder
        .create(&tenant_directory)
        .map_err(|error| format!("credential directory could not be created ({error})"))?;
    let path = tenant_directory.join(name);
    let mut options = fs::OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(path)
        .map_err(|error| format!("credential file could not be opened ({error})"))?;
    file.write_all(value.as_bytes())
        .map_err(|error| format!("credential file could not be written ({error})"))?;
    file.sync_all()
        .map_err(|error| format!("credential file could not be synced ({error})"))
}

fn broker_credential(tenant_id: &str, credential_ref: &str) -> Option<String> {
    let url = std::env::var("MDX_MODEL_SECRET_BROKER_URL").ok()?;
    if !valid_secret_broker_url(&url) {
        return None;
    }
    let mut request = ureq::post(url.trim_end_matches('/'))
        .set("Content-Type", "application/json")
        .set("Accept", "application/json");
    if let Ok(token) = std::env::var("MDX_MODEL_SECRET_BROKER_TOKEN")
        && !token.trim().is_empty()
    {
        request = request.set("Authorization", &format!("Bearer {}", token.trim()));
    }
    let response = request
        .timeout(std::time::Duration::from_secs(3))
        .send_json(serde_json::json!({
            "tenant_id": tenant_id,
            "credential_ref": credential_ref,
        }))
        .ok()?;
    let value: serde_json::Value = response.into_json().ok()?;
    value["value"]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn valid_secret_broker_url(value: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return false;
    }
    if url.scheme() == "https" {
        return url.host_str().is_some();
    }
    url.scheme() == "http" && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"))
}

#[cfg(all(not(test), target_os = "macos"))]
fn keychain_service(tenant_id: &str, env_key: &str) -> String {
    format!(
        "{KEYCHAIN_SERVICE_PREFIX}:{}:{}",
        tenant_id.trim(),
        env_key.trim()
    )
}

#[cfg(all(not(test), target_os = "macos"))]
fn keychain_account(tenant_id: &str, env_key: &str) -> String {
    format!("{}:{}", tenant_id.trim(), env_key.trim())
}

/// The process-wide store the live server uses. Tests construct their own
/// SecretStore instead, so they never touch this and never collide.
pub(crate) fn global() -> &'static SecretStore {
    static STORE: OnceLock<SecretStore> = OnceLock::new();
    STORE.get_or_init(SecretStore::default)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_root(label: &str) -> std::path::PathBuf {
        let unique = format!(
            "mdx-secret-store-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        std::env::temp_dir().join(unique)
    }

    #[test]
    fn session_credential_refs_are_exact_and_tenant_scoped() {
        let store = SecretStore::default();
        store.set_for_tenant("tenant-a", "OPENAI_API_KEY", "tenant-a-secret");
        store.set_for_tenant("tenant-b", "OPENAI_API_KEY", "tenant-b-secret");

        let tenant_a = store
            .credential_for_connection("tenant-a", "secret:session/OPENAI_API_KEY", "IGNORED_KEY")
            .expect("tenant A credential");
        let tenant_b = store
            .credential_for_connection("tenant-b", "secret:session/OPENAI_API_KEY", "IGNORED_KEY")
            .expect("tenant B credential");

        assert_eq!(tenant_a.value, "tenant-a-secret");
        assert_eq!(tenant_b.value, "tenant-b-secret");
        assert_eq!(tenant_a.source, "session");
        assert!(
            store
                .credential_for_connection(
                    "tenant-a",
                    "secret:session/ANTHROPIC_API_KEY",
                    "OPENAI_API_KEY",
                )
                .is_none()
        );
    }

    #[test]
    fn environment_references_accept_canonical_and_legacy_forms() {
        assert_eq!(
            environment_reference_name("secret:environment/OPENAI_API_KEY"),
            Some("OPENAI_API_KEY")
        );
        assert_eq!(
            environment_reference_name("env:ANTHROPIC_API_KEY"),
            Some("ANTHROPIC_API_KEY")
        );
        assert_eq!(
            environment_reference_name("secret:environment/../KEY"),
            None
        );
    }

    #[test]
    fn revoking_a_session_reference_forgets_only_that_tenant() {
        let store = SecretStore::default();
        store.set_for_tenant("tenant-a", "OPENAI_API_KEY", "tenant-a-secret");
        store.set_for_tenant("tenant-b", "OPENAI_API_KEY", "tenant-b-secret");

        assert!(
            store
                .revoke_local_credential_for_connection(
                    "tenant-a",
                    "secret:session/OPENAI_API_KEY",
                    "OPENAI_API_KEY",
                )
                .expect("revoke")
        );
        assert!(
            store
                .credential_for_connection(
                    "tenant-a",
                    "secret:session/OPENAI_API_KEY",
                    "OPENAI_API_KEY",
                )
                .is_none()
        );
        assert!(
            store
                .credential_for_connection(
                    "tenant-b",
                    "secret:session/OPENAI_API_KEY",
                    "OPENAI_API_KEY",
                )
                .is_some()
        );
    }

    #[test]
    fn mounted_credentials_are_tenant_isolated_and_trimmed() {
        let root = temporary_root("mounted");
        let tenant_a = root.join(tenant_directory_name("tenant-a"));
        let tenant_b = root.join(tenant_directory_name("tenant-b"));
        fs::create_dir_all(&tenant_a).expect("tenant A directory");
        fs::create_dir_all(&tenant_b).expect("tenant B directory");
        fs::write(tenant_a.join("OPENAI_API_KEY"), " tenant-a-secret\n").expect("tenant A secret");
        fs::write(tenant_b.join("OPENAI_API_KEY"), "tenant-b-secret").expect("tenant B secret");

        assert_eq!(
            read_mounted_credential(&root, "tenant-a", "OPENAI_API_KEY")
                .expect("tenant A mounted credential"),
            "tenant-a-secret"
        );
        assert_eq!(
            read_mounted_credential(&root, "tenant-b", "OPENAI_API_KEY")
                .expect("tenant B mounted credential"),
            "tenant-b-secret"
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn mounted_credentials_refuse_path_traversal() {
        let root = temporary_root("traversal");
        assert!(read_mounted_credential(&root, "tenant-a", "../secret").is_err());
        assert!(read_managed_credential(&root, "/tmp/secret").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn mounted_credentials_refuse_symlinks() {
        use std::os::unix::fs::symlink;

        let root = temporary_root("symlink");
        let tenant = root.join(tenant_directory_name("tenant-a"));
        fs::create_dir_all(&tenant).expect("tenant directory");
        let target = root.join("target");
        fs::write(&target, "secret").expect("target secret");
        symlink(&target, tenant.join("OPENAI_API_KEY")).expect("credential symlink");

        assert!(read_mounted_credential(&root, "tenant-a", "OPENAI_API_KEY").is_err());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn broker_url_requires_https_or_loopback_http() {
        assert!(valid_secret_broker_url("https://vault.example.test/v1/mdx"));
        assert!(valid_secret_broker_url("http://127.0.0.1:8200/v1/mdx"));
        assert!(!valid_secret_broker_url("http://vault.example.test/v1/mdx"));
        assert!(!valid_secret_broker_url(
            "https://token@vault.example.test/v1/mdx"
        ));
    }
}
