use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug, Eq, PartialEq)]
struct Version(u64, u64, u64);

impl Version {
    fn parse(value: &str) -> Result<Self, String> {
        let clean = value
            .trim()
            .trim_start_matches('v')
            .split('-')
            .next()
            .unwrap_or("");
        let mut parts = clean.split('.');
        let major = parts
            .next()
            .ok_or("missing major version")?
            .parse()
            .map_err(|_| format!("invalid version '{value}'"))?;
        let minor = parts
            .next()
            .unwrap_or("0")
            .parse()
            .map_err(|_| format!("invalid version '{value}'"))?;
        let patch = parts
            .next()
            .unwrap_or("0")
            .parse()
            .map_err(|_| format!("invalid version '{value}'"))?;
        if parts.next().is_some() {
            return Err(format!("invalid version '{value}'"));
        }
        Ok(Self(major, minor, patch))
    }

    fn matches(&self, requirement: &str) -> bool {
        let requirement = requirement.trim();
        if requirement == "*" {
            return true;
        }
        let (operator, raw) = if let Some(v) = requirement.strip_prefix('^') {
            ("^", v)
        } else if let Some(v) = requirement.strip_prefix('~') {
            ("~", v)
        } else if let Some(v) = requirement.strip_prefix(">=") {
            (">=", v)
        } else if let Some(v) = requirement.strip_prefix('=') {
            ("=", v)
        } else {
            ("=", requirement)
        };
        let Ok(base) = Version::parse(raw) else {
            return false;
        };
        match operator {
            "^" if base.0 > 0 => self >= &base && self.0 == base.0,
            "^" if base.1 > 0 => self >= &base && self.0 == 0 && self.1 == base.1,
            "^" => self == &base,
            "~" => self >= &base && self.0 == base.0 && self.1 == base.1,
            ">=" => self >= &base,
            _ => self == &base,
        }
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (&self.0, &self.1, &self.2).cmp(&(&other.0, &other.1, &other.2))
    }
}
impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

#[derive(Clone, Debug)]
enum Source {
    Path(PathBuf),
    Git {
        url: String,
        rev: Option<String>,
    },
    Registry {
        requirement: String,
        registry: Option<String>,
    },
}

#[derive(Clone, Debug)]
struct Dependency {
    name: String,
    source: Source,
}

#[derive(Clone, Debug)]
struct Locked {
    name: String,
    version: String,
    source: String,
    checksum: String,
    cache: PathBuf,
    capabilities: String,
    signature: String,
}

pub fn lock(offline: bool, update: bool) -> Result<String, String> {
    let manifest = fs::read_to_string("ox.toml").map_err(|e| format!("ox.toml: {e}"))?;
    validate_capabilities(&manifest)?;
    let dependencies = parse_dependencies(&manifest)?;
    let cache_root = cache_root()?;
    fs::create_dir_all(&cache_root).map_err(|e| e.to_string())?;
    let existing = if update {
        BTreeMap::new()
    } else {
        parse_lock().unwrap_or_default()
    };
    let mut queue = dependencies;
    let mut resolved: BTreeMap<String, Locked> = BTreeMap::new();
    let mut constraints: BTreeMap<String, Vec<String>> = BTreeMap::new();
    while let Some(dependency) = queue.pop() {
        if let Source::Registry { requirement, .. } = &dependency.source {
            let values = constraints.entry(dependency.name.clone()).or_default();
            if !values.contains(requirement) {
                values.push(requirement.clone());
                values.sort();
            }
        }
        if let Some(package) = resolved.get(&dependency.name) {
            if matches!(dependency.source, Source::Registry { .. }) {
                let selected = Version::parse(&package.version)?;
                let requirements = &constraints[&dependency.name];
                if requirements
                    .iter()
                    .all(|requirement| selected.matches(requirement))
                {
                    continue;
                }
                if offline {
                    return Err(format!(
                        "locked version {} of '{}' violates constraints {}",
                        package.version,
                        dependency.name,
                        requirements.join(", ")
                    ));
                }
                let replacement = resolve(
                    &dependency,
                    false,
                    &cache_root,
                    existing.get(&dependency.name),
                    requirements,
                )?;
                let child_manifest = fs::read_to_string(replacement.cache.join("ox.toml"))
                    .map_err(|e| format!("dependency '{}': {e}", replacement.name))?;
                queue.extend(parse_dependencies(&child_manifest)?);
                resolved.insert(replacement.name.clone(), replacement);
                continue;
            }
            continue;
        }
        let requirements = constraints
            .get(&dependency.name)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let package = resolve(
            &dependency,
            offline,
            &cache_root,
            existing.get(&dependency.name),
            requirements,
        )?;
        let child_manifest = fs::read_to_string(package.cache.join("ox.toml"))
            .map_err(|e| format!("dependency '{}': {e}", package.name))?;
        queue.extend(parse_dependencies(&child_manifest)?);
        resolved.insert(package.name.clone(), package);
    }
    let resolved = resolved.into_values().collect::<Vec<_>>();
    let output = render_lock(&manifest, &resolved);
    if offline {
        let old = fs::read_to_string("ox.lock").map_err(|e| format!("ox.lock: {e}"))?;
        if old != output {
            return Err("ox.lock is stale; run 'ox lock' online".into());
        }
        Ok(format!("Verified {} dependencies offline", resolved.len()))
    } else {
        fs::write("ox.lock", output).map_err(|e| e.to_string())?;
        Ok(format!("Locked {} dependencies", resolved.len()))
    }
}

pub fn add(name: &str, spec: &str) -> Result<String, String> {
    validate_name(name)?;
    let manifest = fs::read_to_string("ox.toml").map_err(|e| format!("ox.toml: {e}"))?;
    let value = if let Some(url) = spec.strip_prefix("git+") {
        format!("{{ git = \"{url}\" }}")
    } else if spec.starts_with("http://") || spec.starts_with("https://") || spec.ends_with(".git")
    {
        format!("{{ git = \"{spec}\" }}")
    } else if spec.starts_with('.') || spec.starts_with('/') {
        format!("{{ path = \"{spec}\" }}")
    } else {
        format!("\"{spec}\"")
    };
    let updated = set_dependency(&manifest, name, Some(&value))?;
    fs::write("ox.toml", updated).map_err(|e| e.to_string())?;
    lock(false, false)?;
    Ok(format!("Added {name}"))
}

pub fn remove(name: &str) -> Result<String, String> {
    let manifest = fs::read_to_string("ox.toml").map_err(|e| format!("ox.toml: {e}"))?;
    let updated = set_dependency(&manifest, name, None)?;
    fs::write("ox.toml", updated).map_err(|e| e.to_string())?;
    lock(false, false)?;
    Ok(format!("Removed {name}"))
}

pub fn vendor() -> Result<String, String> {
    let locked = parse_lock().map_err(|_| "ox.lock is missing; run 'ox lock' first".to_string())?;
    let root = Path::new("vendor");
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    for package in locked.values() {
        let target = root.join(&package.name);
        if target.exists() {
            fs::remove_dir_all(&target).map_err(|e| e.to_string())?;
        }
        copy_tree(&package.cache, &target)?;
    }
    Ok(format!("Vendored {} dependencies", locked.len()))
}

pub fn audit() -> Result<String, String> {
    let locked = parse_lock().map_err(|_| "ox.lock is missing; run 'ox lock' first".to_string())?;
    let advisory = env::var("OX_ADVISORY_DB").unwrap_or_else(|_| ".ox/advisories.txt".into());
    let contents = fs::read_to_string(&advisory).unwrap_or_default();
    let mut findings = Vec::new();
    for line in contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let fields: Vec<_> = line.split('|').map(str::trim).collect();
        if fields.len() < 3 {
            return Err(format!("invalid advisory entry: {line}"));
        }
        if let Some(package) = locked.get(fields[0]) {
            let version = Version::parse(&package.version)?;
            if version.matches(fields[1]) {
                findings.push(format!(
                    "{} {}: {}",
                    package.name, package.version, fields[2]
                ));
            }
        }
    }
    if findings.is_empty() {
        Ok(format!(
            "Audited {} dependencies: no advisories",
            locked.len()
        ))
    } else {
        Err(format!(
            "security advisories found:\n{}",
            findings.join("\n")
        ))
    }
}

pub fn resolve_import(module: &str, project_root: &Path) -> Result<Option<PathBuf>, String> {
    let mut components = module.split('.');
    let Some(package_name) = components.next() else {
        return Ok(None);
    };
    let lock_path = project_root.join("ox.lock");
    let locked = parse_lock_from(&lock_path)?;
    let application =
        fs::read_to_string(project_root.join("ox.toml")).map_err(|e| format!("ox.toml: {e}"))?;
    let lock_source = fs::read_to_string(&lock_path).map_err(|e| e.to_string())?;
    let rendered = render_lock(&application, &locked.values().cloned().collect::<Vec<_>>());
    if lock_source != rendered {
        return Err("ox.lock integrity chain is stale or modified; run 'ox lock'".into());
    }
    let Some(package) = locked.get(package_name) else {
        return Ok(None);
    };
    verify_cached(package)?;
    let granted = capability_set(&application)?;
    for required in package
        .capabilities
        .split(',')
        .filter(|value| !value.is_empty())
    {
        if !granted.contains(required) {
            return Err(format!(
                "package '{}' requires capability '{required}'",
                package.name
            ));
        }
    }
    let path = components
        .map(|value| {
            validate_name(value)?;
            Ok(value)
        })
        .collect::<Result<Vec<_>, String>>()?
        .join("/");
    let candidates = if path.is_empty() {
        vec![
            package.cache.join("src/lib.ox"),
            package.cache.join("lib.ox"),
        ]
    } else {
        vec![
            package.cache.join(format!("src/{path}.ox")),
            package.cache.join(format!("{path}.ox")),
        ]
    };
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .map(Some)
        .ok_or_else(|| format!("module '{module}' does not exist in package '{package_name}'"))
}

pub fn enforce_program_capabilities(source: &str, project_root: &Path) -> Result<(), String> {
    let manifest_path = project_root.join("ox.toml");
    if !manifest_path.exists() {
        return Ok(());
    }
    let manifest = fs::read_to_string(&manifest_path).map_err(|e| e.to_string())?;
    let granted = capability_set(&manifest)?;
    let checks = [
        (
            "network",
            ["http_get(", "http_post(", "http_put(", "http_delete("].as_slice(),
        ),
        ("environment", ["env_get("].as_slice()),
        ("filesystem", ["file_read(", "file_write("].as_slice()),
        ("secrets", ["secret_get("].as_slice()),
    ];
    for (capability, markers) in checks {
        if markers.iter().any(|marker| source.contains(marker)) && !granted.contains(capability) {
            return Err(format!(
                "program uses '{capability}' without granting it in [capabilities]"
            ));
        }
    }
    Ok(())
}

pub fn pack(output: Option<&str>) -> Result<String, String> {
    let root = fs::canonicalize(".").map_err(|e| e.to_string())?;
    let manifest = fs::read_to_string(root.join("ox.toml")).map_err(|e| format!("ox.toml: {e}"))?;
    validate_capabilities(&manifest)?;
    parse_dependencies(&manifest)?;
    let name = package_name(&root)?;
    let version = package_version(&root)?;
    let target = output
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("{name}-{version}.oxpkg")));
    write_archive(&root, &target)?;
    Ok(format!("Packed {}", target.display()))
}

fn write_archive(root: &Path, target: &Path) -> Result<(), String> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort();
    let mut archive = fs::File::create(target).map_err(|e| e.to_string())?;
    archive.write_all(b"OXPKG1\n").map_err(|e| e.to_string())?;
    archive
        .write_all(format!("checksum={}\nfiles={}\n", tree_hash(root)?, files.len()).as_bytes())
        .map_err(|e| e.to_string())?;
    for relative in files {
        if relative == target {
            continue;
        }
        let data = fs::read(root.join(&relative)).map_err(|e| e.to_string())?;
        let name = relative.to_string_lossy();
        archive
            .write_all(format!("{} {}\n", name.len(), data.len()).as_bytes())
            .map_err(|e| e.to_string())?;
        archive
            .write_all(name.as_bytes())
            .map_err(|e| e.to_string())?;
        archive.write_all(&data).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn publish(registry: Option<&str>) -> Result<String, String> {
    let registry = registry
        .map(PathBuf::from)
        .or_else(|| env::var_os("OX_REGISTRY").map(PathBuf::from))
        .ok_or("publish requires --registry PATH or OX_REGISTRY")?;
    if registry.to_string_lossy().contains("://") {
        return Err("alpha.8 publishing requires a local registry checkout".into());
    }
    let root = fs::canonicalize(".").map_err(|e| e.to_string())?;
    let registry = if registry.is_absolute() {
        registry
    } else {
        root.join(registry)
    };
    if registry.starts_with(&root) {
        return Err("registry must be outside the package being published".into());
    }
    let name = package_name(&root)?;
    validate_name(&name)?;
    let version = package_version(&root)?;
    Version::parse(&version)?;
    let target = registry.join(&name).join(&version);
    if target.exists() {
        return Err(format!("package {name} {version} already exists"));
    }
    fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
    copy_tree(&root, &target)?;
    let checksum = tree_hash(&target)?;
    fs::write(target.join(".ox-checksum"), format!("{checksum}\n")).map_err(|e| e.to_string())?;
    let key = env::var("OX_SIGNING_KEY_ID").unwrap_or_else(|_| "unsigned".into());
    let signature = env::var("OX_SIGNATURE").unwrap_or_else(|_| "unsigned".into());
    fs::write(
        target.join(".ox-signature"),
        format!("key={key}\nsignature={signature}\n"),
    )
    .map_err(|e| e.to_string())?;
    write_archive(&target, &target.join(format!("{name}-{version}.oxpkg")))?;
    Ok(format!(
        "Published {name} {version} to {}",
        registry.display()
    ))
}

pub fn workspace() -> Result<String, String> {
    let manifest = fs::read_to_string("ox.toml").map_err(|e| format!("ox.toml: {e}"))?;
    let member_line = section_lines(&manifest, "workspace")
        .into_iter()
        .find(|line| line.starts_with("members"))
        .ok_or("[workspace] requires members = [...] ")?;
    let raw = member_line
        .split_once('=')
        .ok_or("invalid workspace members")?
        .1
        .trim();
    if !raw.starts_with('[') || !raw.ends_with(']') {
        return Err("workspace members must be an array".into());
    }
    let mut names = BTreeSet::new();
    let mut count = 0;
    for value in raw[1..raw.len() - 1]
        .split(',')
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        let member = PathBuf::from(unquote(value)?);
        let canonical = fs::canonicalize(&member)
            .map_err(|e| format!("workspace member {}: {e}", member.display()))?;
        let name = package_name(&canonical)?;
        if !names.insert(name.clone()) {
            return Err(format!("duplicate workspace package '{name}'"));
        }
        let original = env::current_dir().map_err(|e| e.to_string())?;
        env::set_current_dir(&canonical).map_err(|e| e.to_string())?;
        let result = lock(false, false);
        env::set_current_dir(&original).map_err(|e| e.to_string())?;
        result.map_err(|error| format!("workspace package '{name}': {error}"))?;
        count += 1;
    }
    Ok(format!("Resolved {count} workspace packages"))
}

fn resolve(
    dependency: &Dependency,
    offline: bool,
    cache_root: &Path,
    old: Option<&Locked>,
    requirements: &[String],
) -> Result<Locked, String> {
    if offline {
        let package = old
            .ok_or_else(|| format!("dependency '{}' is not locked", dependency.name))?
            .clone();
        verify_cached(&package)?;
        return Ok(package);
    }
    let staging = cache_root.join(format!(".staging-{}", dependency.name));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|e| e.to_string())?;
    }
    let (source, version) = match &dependency.source {
        Source::Path(path) => {
            let absolute = fs::canonicalize(path)
                .map_err(|e| format!("dependency '{}': {e}", dependency.name))?;
            copy_tree(&absolute, &staging)?;
            (
                format!("path+{}", absolute.display()),
                package_version(&staging)?,
            )
        }
        Source::Git { url, rev } => {
            git_clone(url, rev.as_deref(), &staging)?;
            let revision = git_output(&staging, &["rev-parse", "HEAD"])?;
            (format!("git+{url}#{revision}"), package_version(&staging)?)
        }
        Source::Registry {
            requirement: _,
            registry,
        } => {
            let registry = registry
                .clone()
                .or_else(|| env::var("OX_REGISTRY").ok())
                .ok_or_else(|| {
                    format!(
                        "registry dependency '{}' requires OX_REGISTRY or registry = ...",
                        dependency.name
                    )
                })?;
            let index = materialize_registry(&registry, cache_root, offline)?;
            let package_root = index.join(&dependency.name);
            let mut candidates = fs::read_dir(&package_root)
                .map_err(|e| format!("registry package '{}': {e}", dependency.name))?
                .filter_map(Result::ok)
                .filter(|e| e.path().is_dir())
                .filter_map(|e| {
                    Version::parse(&e.file_name().to_string_lossy())
                        .ok()
                        .map(|v| (v, e.path()))
                })
                .filter(|(v, _)| {
                    requirements
                        .iter()
                        .all(|requirement| v.matches(requirement))
                })
                .collect::<Vec<_>>();
            candidates.sort_by(|a, b| b.0.cmp(&a.0));
            let (version, path) = candidates.into_iter().next().ok_or_else(|| {
                format!(
                    "no version of '{}' satisfies constraints {}",
                    dependency.name,
                    requirements.join(", ")
                )
            })?;
            if path.join(".ox-checksum").is_file() {
                let expected =
                    fs::read_to_string(path.join(".ox-checksum")).map_err(|e| e.to_string())?;
                let actual = tree_hash(&path)?;
                if expected.trim() != actual {
                    return Err(format!(
                        "registry checksum mismatch for '{}'",
                        dependency.name
                    ));
                }
            }
            copy_tree(&path, &staging)?;
            let declared = package_version(&staging)?;
            if declared != version.to_string() {
                return Err(format!("registry version mismatch for '{}': directory is {version}, manifest is {declared}", dependency.name));
            }
            (format!("registry+{registry}"), version.to_string())
        }
    };
    let declared_name = package_name(&staging)?;
    if declared_name != dependency.name {
        return Err(format!(
            "dependency confusion rejected: requested '{}', package declares '{declared_name}'",
            dependency.name
        ));
    }
    let checksum = tree_hash(&staging)?;
    let signature = fs::read_to_string(staging.join(".ox-signature"))
        .unwrap_or_else(|_| "unsigned".into())
        .trim()
        .replace('\n', ";");
    let destination = cache_root.join(&checksum);
    if destination.exists() {
        fs::remove_dir_all(&staging).map_err(|e| e.to_string())?;
    } else {
        fs::rename(&staging, &destination).map_err(|e| e.to_string())?;
    }
    let capabilities = capabilities(&destination)?;
    if env::var("OX_REQUIRE_SIGNATURE").as_deref() == Ok("1")
        && (signature == "unsigned" || signature.contains("signature=unsigned"))
    {
        return Err(format!("package '{}' is unsigned", dependency.name));
    }
    if let Ok(trusted) = env::var("OX_TRUSTED_KEYS") {
        let key = signature
            .split(';')
            .find_map(|field| field.strip_prefix("key="))
            .unwrap_or("unsigned");
        if key != "unsigned" && !trusted.split(',').map(str::trim).any(|value| value == key) {
            return Err(format!(
                "package '{}' is signed by untrusted key '{key}'",
                dependency.name
            ));
        }
    }
    Ok(Locked {
        name: dependency.name.clone(),
        version,
        source,
        checksum,
        cache: destination,
        capabilities,
        signature,
    })
}

fn materialize_registry(value: &str, cache_root: &Path, offline: bool) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if path.exists() {
        return fs::canonicalize(path).map_err(|e| e.to_string());
    }
    let destination = cache_root.join(format!("registry-{:016x}", stable_hash(value.as_bytes())));
    if offline {
        return destination
            .exists()
            .then_some(destination)
            .ok_or_else(|| "registry index is not cached".into());
    }
    if destination.exists() {
        run_git(
            Some(&destination),
            &["fetch", "--tags", "--prune", "origin"],
        )?;
        run_git(Some(&destination), &["reset", "--hard", "origin/HEAD"])?;
    } else {
        git_clone(value, None, &destination)?;
    }
    Ok(destination)
}

fn git_clone(url: &str, revision: Option<&str>, destination: &Path) -> Result<(), String> {
    let destination_text = destination.to_string_lossy().into_owned();
    let mut args = vec!["clone", "--quiet", "--depth", "1"];
    if let Some(revision) = revision {
        args.extend(["--branch", revision]);
    }
    args.extend([url, &destination_text]);
    run_git(None, &args)
}

fn run_git(cwd: Option<&Path>, args: &[&str]) -> Result<(), String> {
    let mut command = Command::new("git");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command
        .args(args)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

fn git_output(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

fn parse_dependencies(manifest: &str) -> Result<Vec<Dependency>, String> {
    let section = section_lines(manifest, "dependencies");
    let mut output = Vec::new();
    let mut names = BTreeSet::new();
    for line in section {
        let (name, raw) = line
            .split_once('=')
            .ok_or_else(|| format!("invalid dependency: {line}"))?;
        let name = name.trim();
        validate_name(name)?;
        if !names.insert(name.to_owned()) {
            return Err(format!("duplicate dependency '{name}'"));
        }
        let raw = raw.trim();
        let source = if raw.starts_with('"') {
            Source::Registry {
                requirement: unquote(raw)?,
                registry: None,
            }
        } else if raw.starts_with('{') && raw.ends_with('}') {
            let fields = parse_inline_table(&raw[1..raw.len() - 1])?;
            if let Some(path) = fields.get("path") {
                Source::Path(PathBuf::from(path))
            } else if let Some(git) = fields.get("git") {
                Source::Git {
                    url: git.clone(),
                    rev: fields
                        .get("rev")
                        .cloned()
                        .or_else(|| fields.get("tag").cloned()),
                }
            } else if let Some(version) = fields.get("version") {
                Source::Registry {
                    requirement: version.clone(),
                    registry: fields.get("registry").cloned(),
                }
            } else {
                return Err(format!("dependency '{name}' needs path, git, or version"));
            }
        } else {
            return Err(format!("invalid dependency '{name}'"));
        };
        output.push(Dependency {
            name: name.to_owned(),
            source,
        });
    }
    Ok(output)
}

fn validate_capabilities(manifest: &str) -> Result<(), String> {
    let allowed = ["network", "filesystem", "environment", "secrets"];
    for line in section_lines(manifest, "capabilities") {
        let (name, value) = line
            .split_once('=')
            .ok_or_else(|| format!("invalid capability: {line}"))?;
        let name = name.trim();
        let value = value.trim();
        if !allowed.contains(&name) {
            return Err(format!("unknown capability '{name}'"));
        }
        if value != "true" && value != "false" {
            return Err(format!("capability '{name}' must be true or false"));
        }
    }
    Ok(())
}

fn capabilities(root: &Path) -> Result<String, String> {
    let manifest = fs::read_to_string(root.join("ox.toml"))
        .map_err(|e| format!("{}: {e}", root.join("ox.toml").display()))?;
    validate_capabilities(&manifest)?;
    let mut values = section_lines(&manifest, "capabilities")
        .into_iter()
        .filter_map(|line| {
            let (name, value) = line.split_once('=')?;
            (value.trim() == "true").then(|| name.trim().to_owned())
        })
        .collect::<Vec<_>>();
    values.sort();
    Ok(values.join(","))
}

fn package_version(root: &Path) -> Result<String, String> {
    let manifest = fs::read_to_string(root.join("ox.toml"))
        .map_err(|e| format!("{}: {e}", root.join("ox.toml").display()))?;
    for line in section_lines(&manifest, "package") {
        if let Some(("version", value)) = line.split_once('=').map(|(a, b)| (a.trim(), b.trim())) {
            return unquote(value);
        }
    }
    Err(format!(
        "{} has no package version",
        root.join("ox.toml").display()
    ))
}

fn package_name(root: &Path) -> Result<String, String> {
    let manifest = fs::read_to_string(root.join("ox.toml"))
        .map_err(|e| format!("{}: {e}", root.join("ox.toml").display()))?;
    for line in section_lines(&manifest, "package") {
        if let Some(("name", value)) = line.split_once('=').map(|(a, b)| (a.trim(), b.trim())) {
            return unquote(value);
        }
    }
    Err(format!(
        "{} has no package name",
        root.join("ox.toml").display()
    ))
}

fn render_lock(manifest: &str, packages: &[Locked]) -> String {
    let mut chain = 0xcbf29ce484222325_u64;
    for package in packages {
        chain = hash_extend(chain, package.name.as_bytes());
        chain = hash_extend(chain, package.version.as_bytes());
        chain = hash_extend(chain, package.checksum.as_bytes());
        chain = hash_extend(chain, package.signature.as_bytes());
    }
    let mut output = format!(
        "# Generated by Ox; do not edit.\nversion = 3\nmanifest = \"{:016x}\"\nresolution = \"{chain:016x}\"\n",
        stable_hash(manifest.as_bytes()),
    );
    for package in packages {
        output.push_str(&format!("\n[[package]]\nname = {:?}\nversion = {:?}\nsource = {:?}\nchecksum = {:?}\ncache = {:?}\ncapabilities = {:?}\nsignature = {:?}\n", package.name, package.version, package.source, package.checksum, package.cache.to_string_lossy(), package.capabilities, package.signature));
    }
    output
}

fn parse_lock() -> Result<BTreeMap<String, Locked>, String> {
    parse_lock_from(Path::new("ox.lock"))
}

fn parse_lock_from(path: &Path) -> Result<BTreeMap<String, Locked>, String> {
    let contents = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut values = BTreeMap::new();
    for block in contents.split("[[package]]").skip(1) {
        let fields = block
            .lines()
            .filter_map(|line| {
                line.split_once('=')
                    .map(|(a, b)| (a.trim().to_owned(), b.trim().trim_matches('"').to_owned()))
            })
            .collect::<BTreeMap<_, _>>();
        let get = |key: &str| {
            fields
                .get(key)
                .cloned()
                .ok_or_else(|| format!("lock package missing {key}"))
        };
        let package = Locked {
            name: get("name")?,
            version: get("version")?,
            source: get("source")?,
            checksum: get("checksum")?,
            cache: PathBuf::from(get("cache")?),
            capabilities: get("capabilities")?,
            signature: fields
                .get("signature")
                .cloned()
                .unwrap_or_else(|| "unsigned".into()),
        };
        values.insert(package.name.clone(), package);
    }
    Ok(values)
}

fn verify_cached(package: &Locked) -> Result<(), String> {
    if !package.cache.exists() {
        return Err(format!("dependency '{}' is not cached", package.name));
    }
    let actual = tree_hash(&package.cache)?;
    if actual != package.checksum {
        return Err(format!(
            "checksum mismatch for '{}': expected {}, got {actual}",
            package.name, package.checksum
        ));
    }
    Ok(())
}

fn cache_root() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("OX_CACHE_HOME") {
        return Ok(PathBuf::from(path));
    }
    let home = env::var_os("HOME").ok_or("HOME or OX_CACHE_HOME is required")?;
    Ok(PathBuf::from(home).join(".ox/cache/v2"))
}

fn tree_hash(root: &Path) -> Result<String, String> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort();
    let mut state = 0xcbf29ce484222325_u64;
    for relative in files {
        state = hash_extend(state, relative.to_string_lossy().as_bytes());
        state = hash_extend(
            state,
            &fs::read(root.join(&relative)).map_err(|e| e.to_string())?,
        );
    }
    Ok(format!("fnv1a64:{state:016x}"))
}

fn collect_files(root: &Path, current: &Path, output: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(current).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = entry.file_name();
        if entry.file_type().map_err(|e| e.to_string())?.is_symlink() {
            return Err(format!(
                "symbolic links are not allowed in packages: {}",
                path.display()
            ));
        }
        if name == ".git"
            || name == ".ox"
            || name == "target"
            || name == ".ox-checksum"
            || name == ".ox-signature"
            || name.to_string_lossy().ends_with(".oxpkg")
        {
            continue;
        }
        if path.is_dir() {
            collect_files(root, &path, output)?;
        } else if path.is_file() {
            output.push(path.strip_prefix(root).unwrap().to_owned());
        }
    }
    Ok(())
}

fn copy_tree(source: &Path, target: &Path) -> Result<(), String> {
    fs::create_dir_all(target).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(source).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        if entry.file_type().map_err(|e| e.to_string())?.is_symlink() {
            return Err(format!(
                "symbolic links are not allowed in packages: {}",
                entry.path().display()
            ));
        }
        if name == ".git"
            || name == ".ox"
            || name == "target"
            || name.to_string_lossy().ends_with(".oxpkg")
        {
            continue;
        }
        let destination = target.join(&name);
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &destination)?;
        } else {
            fs::copy(entry.path(), destination).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn section_lines<'a>(manifest: &'a str, section: &str) -> Vec<&'a str> {
    let header = format!("[{section}]");
    let mut active = false;
    let mut output = Vec::new();
    for raw in manifest.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            active = line == header;
            continue;
        }
        if active && !line.is_empty() && !line.starts_with('#') {
            output.push(line);
        }
    }
    output
}

fn parse_inline_table(value: &str) -> Result<BTreeMap<String, String>, String> {
    let mut output = BTreeMap::new();
    for field in value.split(',').map(str::trim).filter(|v| !v.is_empty()) {
        let (name, value) = field
            .split_once('=')
            .ok_or_else(|| format!("invalid dependency field '{field}'"))?;
        output.insert(name.trim().to_owned(), unquote(value.trim())?);
    }
    Ok(output)
}

fn unquote(value: &str) -> Result<String, String> {
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        Ok(value[1..value.len() - 1].to_owned())
    } else {
        Err(format!("expected quoted string, got '{value}'"))
    }
}

fn validate_name(name: &str) -> Result<(), String> {
    if !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        Ok(())
    } else {
        Err(format!("invalid package name '{name}'"))
    }
}

fn capability_set(manifest: &str) -> Result<BTreeSet<String>, String> {
    validate_capabilities(manifest)?;
    Ok(section_lines(manifest, "capabilities")
        .into_iter()
        .filter_map(|line| {
            let (name, value) = line.split_once('=')?;
            (value.trim() == "true").then(|| name.trim().to_owned())
        })
        .collect())
}

fn set_dependency(manifest: &str, name: &str, value: Option<&str>) -> Result<String, String> {
    let mut lines = manifest.lines().map(str::to_owned).collect::<Vec<_>>();
    let start = lines
        .iter()
        .position(|line| line.trim() == "[dependencies]")
        .ok_or("ox.toml has no [dependencies] section")?;
    let end = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, line)| line.trim().starts_with('['))
        .map(|(i, _)| i)
        .unwrap_or(lines.len());
    let prefix = format!("{name} ");
    let existing = (start + 1..end)
        .find(|&i| lines[i].trim_start().starts_with(&prefix) && lines[i].contains('='));
    match (existing, value) {
        (Some(index), Some(value)) => lines[index] = format!("{name} = {value}"),
        (Some(index), None) => {
            lines.remove(index);
        }
        (None, Some(value)) => lines.insert(end, format!("{name} = {value}")),
        (None, None) => return Err(format!("dependency '{name}' is not declared")),
    }
    Ok(format!("{}\n", lines.join("\n")))
}

fn stable_hash(value: &[u8]) -> u64 {
    hash_extend(0xcbf29ce484222325_u64, value)
}
fn hash_extend(mut state: u64, value: &[u8]) -> u64 {
    for byte in value {
        state = (state ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_versions_match_ranges() {
        let version = Version::parse("1.4.2").unwrap();
        assert!(version.matches("^1.2.0"));
        assert!(version.matches("~1.4.0"));
        assert!(!version.matches("^2.0.0"));
        assert!(!Version::parse("0.3.0").unwrap().matches("^0.2.0"));
    }

    #[test]
    fn dependency_sources_parse() {
        let manifest = "[dependencies]\na = \"^1.0.0\"\nb = { path = \"../b\" }\nc = { git = \"https://example/c.git\", rev = \"v1.0.0\" }\n";
        let parsed = parse_dependencies(manifest).unwrap();
        assert_eq!(parsed.len(), 3);
        assert!(matches!(parsed[0].source, Source::Registry { .. }));
        assert!(matches!(parsed[1].source, Source::Path(_)));
        assert!(matches!(parsed[2].source, Source::Git { .. }));
    }

    #[test]
    fn capabilities_are_least_privilege() {
        assert!(validate_capabilities("[capabilities]\nnetwork = false\nsecrets = true\n").is_ok());
        assert!(validate_capabilities("[capabilities]\nshell = true\n").is_err());
    }
}
