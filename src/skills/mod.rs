use serde::{Deserialize, Serialize};

use crate::paths;

/// 已安装的 Skill 记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    /// GitHub 仓库 "owner/repo"
    pub repo: String,
    /// 安装的目录名
    pub directory: String,
    /// 安装时间（Unix timestamp）
    pub installed_at: i64,
}

/// Skills 存储文件结构
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillStore {
    pub skills: Vec<InstalledSkill>,
}

impl SkillStore {
    pub fn load() -> anyhow::Result<Self> {
        let path = paths::skills_config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&raw).unwrap_or_default())
    }

    pub fn save(&self) -> anyhow::Result<()> {
        paths::ensure_helper_dirs()?;
        let path = paths::skills_config_path()?;
        let raw = serde_json::to_string_pretty(self)?;
        crate::config::write_atomic(&path, &raw)
    }

    pub fn find(&self, skill_id: &str) -> Option<&InstalledSkill> {
        self.skills.iter().find(|s| s.id == skill_id)
    }

    pub fn add(&mut self, skill: InstalledSkill) {
        self.skills.retain(|s| s.id != skill.id);
        self.skills.push(skill);
    }

    pub fn remove(&mut self, skill_id: &str) -> bool {
        let len = self.skills.len();
        self.skills.retain(|s| s.id != skill_id);
        self.skills.len() < len
    }
}

/// 从 GitHub 仓库安装 Skill。
/// repo 格式: "owner/repo" 或 "owner/repo:branch"
/// 支持子目录: "owner/repo/subdir"（用 : 分隔 branch）
pub async fn install_skill(repo: &str) -> anyhow::Result<InstalledSkill> {
    let (owner, repo_name, branch, subdir) = parse_repo(repo)?;
    let skill_id = if subdir.is_empty() {
        format!("{owner}/{repo_name}")
    } else {
        format!("{owner}/{repo_name}/{subdir}")
    };

    // 检查是否已安装
    let mut store = SkillStore::load()?;
    if store.find(&skill_id).is_some() {
        anyhow::bail!("Skill {skill_id} 已安装，请先卸载");
    }

    let skills_dir = paths::skills_dir()?;
    std::fs::create_dir_all(&skills_dir)?;

    let install_dir = skills_dir.join(if subdir.is_empty() {
        repo_name.clone()
    } else {
        format!("{repo_name}_{subdir}").replace('/', "_")
    });

    // 检查是否已存在目录（可能是手动放的）
    if install_dir.exists() {
        anyhow::bail!("目录已存在: {}", install_dir.display());
    }

    // 使用 git clone 获取 skill
    let branch_arg = if branch.is_empty() {
        String::new()
    } else {
        format!("--branch={branch}")
    };

    let clone_url = format!("https://github.com/{owner}/{repo_name}.git");
    let mut cmd = std::process::Command::new("git");
    cmd.arg("clone")
        .arg("--depth=1")
        .arg(&clone_url)
        .arg(&install_dir);
    if !branch_arg.is_empty() {
        cmd.arg(&branch_arg);
    }

    let output = cmd.output().map_err(|e| {
        anyhow::anyhow!("git clone 失败: {e}。请确认已安装 git 且网络可达 github.com")
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git clone 失败:\n{stderr}");
    }

    // 如果有子目录，实际 skill 在子目录中
    let skill_dir = if subdir.is_empty() {
        install_dir.clone()
    } else {
        install_dir.join(&subdir)
    };

    if !skill_dir.exists() {
        std::fs::remove_dir_all(&install_dir).ok();
        anyhow::bail!("Skill 子目录不存在: {subdir}");
    }

    // 同步到所有支持的工具
    sync_to_all_tools(&skill_id, &skill_dir)?;

    let skill = InstalledSkill {
        id: skill_id,
        name: if subdir.is_empty() {
            repo_name.clone()
        } else {
            subdir.to_string()
        },
        description: format!("从 {owner}/{repo_name} 安装"),
        repo: format!("{owner}/{repo_name}"),
        directory: skill_dir.to_string_lossy().to_string(),
        installed_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64,
    };

    store.add(skill.clone());
    store.save()?;

    Ok(skill)
}

/// 卸载 Skill
pub fn uninstall_skill(skill_id: &str) -> anyhow::Result<()> {
    let mut store = SkillStore::load()?;
    let skill = store
        .find(skill_id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("未找到 Skill: {skill_id}"))?;

    // 删除安装目录
    let dir = std::path::PathBuf::from(&skill.directory);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }

    // 从所有工具的 skills 目录中移除
    remove_from_claude_code(skill_id)?;

    store.remove(skill_id);
    store.save()?;

    Ok(())
}

/// 列出所有已安装的 Skill
pub fn list_skills() -> anyhow::Result<Vec<InstalledSkill>> {
    let store = SkillStore::load()?;
    Ok(store.skills)
}

/// 同步所有已安装的 Skill 到所有支持的工具
pub fn sync_all_to_tools() -> anyhow::Result<()> {
    let store = SkillStore::load()?;
    for skill in &store.skills {
        let dir = std::path::PathBuf::from(&skill.directory);
        if dir.exists() {
            sync_to_all_tools(&skill.id, &dir)?;
        }
    }
    Ok(())
}

/// 同步单个 Skill 到所有支持的工具目录
fn sanitize_skill_dir_name(skill_id: &str) -> String {
    skill_id
        .replace('/', "_")
        .replace(':', "_")
        .replace('\\', "_")
        .replace(' ', "_")
}

fn sync_to_target(target_dir: &std::path::Path, skill_id: &str, source_dir: &std::path::Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(target_dir)?;
    let target = target_dir.join(sanitize_skill_dir_name(skill_id));

    // 删除旧链接/目录
    if target.exists() {
        if target.is_symlink() {
            std::fs::remove_file(&target)?;
        } else {
            std::fs::remove_dir_all(&target)?;
        }
    }

    // 创建符号链接（Windows 上需要权限，失败则复制）
    #[cfg(windows)]
    {
        if std::os::windows::fs::symlink_dir(source_dir, &target).is_err() {
            copy_dir_recursive(source_dir, &target)?;
        }
    }

    #[cfg(not(windows))]
    {
        std::os::unix::fs::symlink(source_dir, &target)?;
    }

    Ok(())
}

fn sync_to_all_tools(skill_id: &str, source_dir: &std::path::Path) -> anyhow::Result<()> {
    for (_name, dir) in paths::all_skills_dirs() {
        sync_to_target(&dir, skill_id, source_dir)?;
    }
    Ok(())
}

/// 从所有工具的 skills 目录移除
fn remove_from_claude_code(skill_id: &str) -> anyhow::Result<()> {
    let sanitized = sanitize_skill_dir_name(skill_id);
    for (_name, dir) in paths::all_skills_dirs() {
        let target = dir.join(&sanitized);
        if target.exists() {
            if target.is_symlink() {
                std::fs::remove_file(&target)?;
            } else {
                std::fs::remove_dir_all(&target)?;
            }
        }
    }
    Ok(())
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// 从本地文件夹安装 Skill。
pub fn install_local_skill(path: &str) -> anyhow::Result<InstalledSkill> {
    let source = std::path::PathBuf::from(path.trim());
    if !source.exists() {
        anyhow::bail!("路径不存在: {}", source.display());
    }
    if !source.is_dir() {
        anyhow::bail!("路径不是文件夹: {}", source.display());
    }

    // 检查是否包含 SKILL.md（提示但不强制）
    let has_skill_md = source.join("SKILL.md").exists();

    // 用目录名作为 skill id
    let dir_name = source
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "local-skill".to_string());

    let skill_id = format!("local:{}", dir_name);

    // 检查是否已安装
    let mut store = SkillStore::load()?;
    if store.find(&skill_id).is_some() {
        anyhow::bail!("本地 Skill \"{}\" 已安装，请先卸载", dir_name);
    }

    let skills_dir = paths::skills_dir()?;
    std::fs::create_dir_all(&skills_dir)?;
    let install_dir = skills_dir.join(format!("local_{dir_name}"));

    // 如果安装目录已存在（可能之前手动放的），直接使用
    if install_dir.exists() {
        anyhow::bail!("安装目录已存在: {}，请先手动删除", install_dir.display());
    }

    // 复制到 skills 目录
    copy_dir_recursive(&source, &install_dir)?;

    // 同步到所有支持的工具
    sync_to_all_tools(&skill_id, &install_dir)?;

    let description = if has_skill_md {
        format!("本地 Skill（{}）", source.display())
    } else {
        format!("本地文件夹（{}）", source.display())
    };

    let skill = InstalledSkill {
        id: skill_id,
        name: dir_name,
        description,
        repo: String::new(),
        directory: install_dir.to_string_lossy().to_string(),
        installed_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64,
    };

    store.add(skill.clone());
    store.save()?;

    Ok(skill)
}

/// 解析 repo 字符串: "owner/repo", "owner/repo:branch", "owner/repo/subdir"
fn parse_repo(raw: &str) -> anyhow::Result<(String, String, String, String)> {
    let raw = raw.trim().trim_end_matches('/');
    let (rest, branch) = if let Some((r, b)) = raw.rsplit_once(':') {
        (r, b.to_string())
    } else {
        (raw, String::new())
    };

    let parts: Vec<&str> = rest.split('/').collect();
    if parts.len() < 2 {
        anyhow::bail!("格式错误：需要 owner/repo，例如 anthropics/skills");
    }
    let owner = parts[0].to_string();
    let repo_name = parts[1].to_string();
    let subdir = if parts.len() > 2 {
        parts[2..].join("/")
    } else {
        String::new()
    };

    if owner.is_empty() || repo_name.is_empty() {
        anyhow::bail!("owner 和 repo 不能为空");
    }

    Ok((owner, repo_name, branch, subdir))
}
