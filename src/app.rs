use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::{
    config::Config,
    remote::{Remote, RemoteEntry},
    ui,
};

#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub modified: String,
}

#[derive(Clone)]
pub enum Location {
    Local(PathBuf),
    Profiles,
    RemoteRoot {
        remote: Remote,
    },
    Remote {
        remote: Remote,
        bucket: String,
        prefix: String,
    },
}

#[derive(Clone)]
pub struct Panel {
    pub location: Location,
    pub entries: Vec<Entry>,
    pub selected: usize,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Normal,
    Help,
    ConfirmDelete,
    /// 원격 프로필이 없을 때 설정 방법을 안내한다.
    RemoteSetup,
}

pub struct App {
    pub panels: [Panel; 2],
    pub active: usize,
    pub running: bool,
    pub mode: Mode,
    pub status: String,
    pub config: Config,
    /// config.toml을 읽지 못했을 때의 오류 메시지. 안내 dialog에 표시한다.
    pub config_error: Option<String>,
    /// 직전 동작의 오류. 다음 키 입력 때 지워지며, 상태줄에 선택 항목 대신 표시된다.
    pub error: Option<String>,
    pub search: String,
    pub right_local_path: PathBuf,
}

impl App {
    pub fn new() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let (config, config_error) = match Config::load() {
            Ok(config) => (config, None),
            Err(err) => (Config::default(), Some(format!("{err:#}"))),
        };
        let panel = Panel {
            location: Location::Local(cwd.clone()),
            entries: vec![],
            selected: 0,
        };
        Self {
            panels: [panel.clone(), panel],
            active: 0,
            running: true,
            mode: Mode::Normal,
            status: "R Mdir — F1 도움말 · Tab 패널 전환".into(),
            config,
            config_error,
            error: None,
            search: String::new(),
            right_local_path: cwd,
        }
    }

    pub async fn run(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<()> {
        self.refresh(0).await?;
        self.refresh(1).await?;
        while self.running {
            terminal.draw(|f| ui::draw(f, self))?;
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    self.handle_key(key).await;
                }
            }
        }
        Ok(())
    }

    async fn handle_key(&mut self, key: KeyEvent) {
        self.error = None;
        if matches!(self.mode, Mode::Help | Mode::RemoteSetup) {
            self.mode = Mode::Normal;
            return;
        }
        if self.mode == Mode::ConfirmDelete {
            self.mode = Mode::Normal;
            if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
                self.delete_selected().await;
            }
            return;
        }
        let result = match key.code {
            KeyCode::Esc => {
                self.running = false;
                Ok(())
            }
            KeyCode::Char('q') if self.search.is_empty() => {
                self.running = false;
                Ok(())
            }
            KeyCode::F(10) => {
                self.running = false;
                Ok(())
            }
            KeyCode::Tab => {
                self.active = 1 - self.active;
                self.search.clear();
                Ok(())
            }
            KeyCode::Up => {
                self.move_selection(-1);
                Ok(())
            }
            KeyCode::Down => {
                self.move_selection(1);
                Ok(())
            }
            KeyCode::Home => {
                self.panels[self.active].selected = 0;
                Ok(())
            }
            KeyCode::End => {
                let n = self.panels[self.active].entries.len();
                self.panels[self.active].selected = n.saturating_sub(1);
                Ok(())
            }
            KeyCode::Enter | KeyCode::Right => self.enter().await,
            KeyCode::Backspace if !self.search.is_empty() => {
                self.search.pop();
                self.select_search_match();
                Ok(())
            }
            KeyCode::Backspace | KeyCode::Left => self.up().await,
            KeyCode::F(1) | KeyCode::Char('?') => {
                self.mode = Mode::Help;
                Ok(())
            }
            KeyCode::F(2) if self.config.remote.is_empty() => {
                self.mode = Mode::RemoteSetup;
                Ok(())
            }
            KeyCode::F(2) => self.toggle_remote_right().await,
            KeyCode::F(5) => self.copy_or_download().await,
            KeyCode::F(7) => self.mkdir().await,
            KeyCode::F(8) => {
                let panel = &self.panels[self.active];
                match panel.entries.get(panel.selected) {
                    None => Err(anyhow!("선택된 항목이 없습니다")),
                    Some(entry) if entry.name == ".." => {
                        Err(anyhow!("상위 폴더는 삭제할 수 없습니다"))
                    }
                    Some(_) if !matches!(panel.location, Location::Local(_)) => {
                        Err(anyhow!("원격 저장소는 읽기 전용입니다"))
                    }
                    Some(_) => {
                        self.mode = Mode::ConfirmDelete;
                        Ok(())
                    }
                }
            }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.refresh(self.active).await
            }
            KeyCode::Char(character)
                if character.is_ascii_alphanumeric()
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.search.push(character.to_ascii_lowercase());
                self.select_search_match();
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(e) = result {
            self.error = Some(format!("오류: {e:#}"));
        }
    }

    fn move_selection(&mut self, delta: isize) {
        self.search.clear();
        let p = &mut self.panels[self.active];
        if p.entries.is_empty() {
            return;
        }
        p.selected = (p.selected as isize + delta).clamp(0, p.entries.len() as isize - 1) as usize;
    }

    fn select_search_match(&mut self) {
        if self.search.is_empty() {
            return;
        }
        let query = self.search.to_ascii_lowercase();
        let panel = &mut self.panels[self.active];
        if let Some(index) = panel.entries.iter().position(|entry| {
            entry.name != ".." && entry.name.to_ascii_lowercase().starts_with(&query)
        }) {
            panel.selected = index;
            self.status = format!("빠른 이동: {}", panel.entries[index].name);
        } else {
            self.status = format!("일치하는 항목 없음: {}", self.search);
        }
    }

    /// 패널을 새로 고치고, 반대편 패널이 같은 로컬 폴더를 보고 있으면 그쪽도 함께 새로 고친다.
    async fn refresh_with_twin(&mut self, idx: usize) -> Result<()> {
        let other = 1 - idx;
        let same_dir = matches!(
            (&self.panels[idx].location, &self.panels[other].location),
            (Location::Local(a), Location::Local(b)) if a == b
        );
        self.refresh(idx).await?;
        if same_dir {
            self.refresh(other).await?;
        }
        Ok(())
    }

    pub async fn refresh(&mut self, idx: usize) -> Result<()> {
        let loc = self.panels[idx].location.clone();
        let show_parent = match &loc {
            Location::Local(path) => path.parent().is_some(),
            Location::Profiles => false,
            Location::RemoteRoot { .. } | Location::Remote { .. } => true,
        };
        let mut entries = match loc {
            Location::Local(path) => local_entries(&path)?,
            Location::Profiles => self
                .config
                .remote
                .keys()
                .map(|name| Entry {
                    name: name.clone(),
                    path: name.into(),
                    is_dir: true,
                    size: 0,
                    modified: "S3 호환 원격 저장소".into(),
                })
                .collect(),
            Location::RemoteRoot { remote } => remote
                .buckets()
                .await?
                .into_iter()
                .map(remote_entry)
                .collect(),
            Location::Remote {
                remote,
                bucket,
                prefix,
            } => remote
                .list(&bucket, &prefix)
                .await?
                .into_iter()
                .map(remote_entry)
                .collect(),
        };
        if show_parent {
            entries.insert(
                0,
                Entry {
                    name: "..".into(),
                    path: PathBuf::from(".."),
                    is_dir: true,
                    size: 0,
                    modified: String::new(),
                },
            );
        }
        let p = &mut self.panels[idx];
        p.entries = entries;
        p.selected = p.selected.min(p.entries.len().saturating_sub(1));
        Ok(())
    }

    async fn enter(&mut self) -> Result<()> {
        self.search.clear();
        let idx = self.active;
        let Some(entry) = self.panels[idx]
            .entries
            .get(self.panels[idx].selected)
            .cloned()
        else {
            return Ok(());
        };
        if entry.name == ".." {
            return self.up().await;
        }
        if !entry.is_dir {
            self.status = format!("{} · {}", entry.name, human_size(entry.size));
            return Ok(());
        }
        self.panels[idx].location = match self.panels[idx].location.clone() {
            Location::Local(_) => Location::Local(entry.path),
            Location::Profiles => {
                let cfg = self
                    .config
                    .remote
                    .get(&entry.name)
                    .context("프로필을 찾을 수 없습니다")?
                    .clone();
                Location::RemoteRoot {
                    remote: Remote::connect(entry.name, cfg).await?,
                }
            }
            Location::RemoteRoot { remote } => Location::Remote {
                remote,
                bucket: entry.name,
                prefix: String::new(),
            },
            Location::Remote { remote, bucket, .. } => Location::Remote {
                remote,
                bucket,
                prefix: entry.path.to_string_lossy().into(),
            },
        };
        self.panels[idx].selected = 0;
        self.refresh(idx).await
    }

    async fn up(&mut self) -> Result<()> {
        self.search.clear();
        let idx = self.active;
        self.panels[idx].location = match self.panels[idx].location.clone() {
            Location::Local(path) => Location::Local(path.parent().unwrap_or(&path).to_path_buf()),
            Location::Profiles => Location::Profiles,
            Location::RemoteRoot { .. } => Location::Profiles,
            Location::Remote {
                remote,
                bucket: _,
                prefix,
            } if prefix.is_empty() => Location::RemoteRoot { remote },
            Location::Remote {
                remote,
                bucket,
                prefix,
            } => {
                let trimmed = prefix.trim_end_matches('/');
                let parent = trimmed
                    .rsplit_once('/')
                    .map(|(p, _)| format!("{p}/"))
                    .unwrap_or_default();
                Location::Remote {
                    remote,
                    bucket,
                    prefix: parent,
                }
            }
        };
        self.panels[idx].selected = 0;
        self.refresh(idx).await
    }

    async fn toggle_remote_right(&mut self) -> Result<()> {
        self.search.clear();
        const RIGHT: usize = 1;
        self.panels[RIGHT].location = match self.panels[RIGHT].location.clone() {
            Location::Local(path) => {
                self.right_local_path = path;
                Location::Profiles
            }
            Location::Profiles | Location::RemoteRoot { .. } | Location::Remote { .. } => {
                Location::Local(self.right_local_path.clone())
            }
        };
        self.panels[RIGHT].selected = 0;
        self.active = RIGHT;
        self.refresh(RIGHT).await
    }

    async fn copy_or_download(&mut self) -> Result<()> {
        let src_idx = self.active;
        let dst_idx = 1 - src_idx;
        let src = self.panels[src_idx]
            .entries
            .get(self.panels[src_idx].selected)
            .context("선택된 항목이 없습니다")?
            .clone();
        let Location::Local(dst_dir) = self.panels[dst_idx].location.clone() else {
            bail!("대상 패널은 로컬 폴더여야 합니다")
        };
        if src.is_dir {
            bail!("현재 버전에서 폴더 복사는 지원하지 않습니다")
        }
        let dst = dst_dir.join(&src.name);
        match self.panels[src_idx].location.clone() {
            Location::Local(_) => {
                if dst.exists() {
                    bail!("대상 파일이 이미 있습니다")
                }
                fs::copy(&src.path, &dst).context("파일 복사 실패")?;
            }
            Location::Remote { remote, bucket, .. } => {
                remote
                    .download(&bucket, &src.path.to_string_lossy(), &dst)
                    .await?
            }
            _ => bail!("이 위치에서는 복사할 수 없습니다"),
        }
        self.status = format!("복사 완료 → {}", dst.display());
        self.refresh(dst_idx).await
    }

    async fn mkdir(&mut self) -> Result<()> {
        let Location::Local(dir) = self.panels[self.active].location.clone() else {
            bail!("원격 저장소는 읽기 전용입니다")
        };
        let mut n = 1;
        let mut path = dir.join("새 폴더");
        while path.exists() {
            n += 1;
            path = dir.join(format!("새 폴더 {n}"));
        }
        fs::create_dir(&path)?;
        self.status = format!("폴더 생성: {}", path.display());
        self.refresh_with_twin(self.active).await
    }

    async fn delete_selected(&mut self) {
        let result: Result<()> = (|| {
            let p = &self.panels[self.active];
            let Location::Local(_) = p.location else {
                bail!("원격 저장소는 읽기 전용입니다")
            };
            let e = p
                .entries
                .get(p.selected)
                .context("선택된 항목이 없습니다")?;
            // ".."의 path는 상대 경로라 지우면 작업 디렉토리의 상위가 날아간다.
            if e.name == ".." || !e.path.is_absolute() {
                bail!("상위 폴더는 삭제할 수 없습니다")
            }
            if e.is_dir {
                let mut children = fs::read_dir(&e.path)
                    .with_context(|| format!("{} 열기 실패", e.path.display()))?;
                if children.next().is_some() {
                    bail!("비어 있지 않은 폴더는 삭제할 수 없습니다: {}", e.name)
                }
                fs::remove_dir(&e.path)?
            } else {
                fs::remove_file(&e.path)?
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.status = "삭제했습니다".into();
                let _ = self.refresh_with_twin(self.active).await;
            }
            Err(e) => self.error = Some(format!("오류: {e:#}")),
        }
    }
}

fn remote_entry(e: RemoteEntry) -> Entry {
    Entry {
        name: e.name,
        path: e.key.into(),
        is_dir: e.is_dir,
        size: e.size,
        modified: e.modified,
    }
}

fn local_entries(path: &Path) -> Result<Vec<Entry>> {
    let mut out = Vec::new();
    for item in fs::read_dir(path).with_context(|| format!("{} 열기 실패", path.display()))? {
        let item = item?;
        let meta = item.metadata()?;
        let modified = meta
            .modified()
            .ok()
            .map(|t| {
                chrono::DateTime::<chrono::Local>::from(t)
                    .format("%y-%m-%d %H:%M")
                    .to_string()
            })
            .unwrap_or_default();
        out.push(Entry {
            name: item.file_name().to_string_lossy().into(),
            path: item.path(),
            is_dir: meta.is_dir(),
            size: meta.len(),
            modified,
        });
    }
    out.sort_by_key(|e| (!e.is_dir, e.name.to_lowercase()));
    Ok(out)
}

pub fn human_size(size: u64) -> String {
    const U: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut n = size as f64;
    let mut i = 0;
    while n >= 1024.0 && i < 4 {
        n /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{size}B")
    } else {
        format!("{n:.1}{}", U[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_file_sizes() {
        assert_eq!(human_size(42), "42B");
        assert_eq!(human_size(1024), "1.0K");
        assert_eq!(human_size(1_572_864), "1.5M");
    }

    #[test]
    fn local_entries_put_directories_first() {
        let root = std::env::temp_dir().join(format!("mdir-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("z-folder")).unwrap();
        fs::write(root.join("a-file.txt"), b"hello").unwrap();
        let entries = local_entries(&root).unwrap();
        assert_eq!(entries[0].name, "z-folder");
        assert!(entries[0].is_dir);
        assert_eq!(entries[1].size, 5);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn refresh_adds_parent_link_first() {
        let mut app = App::new();
        app.refresh(0).await.unwrap();
        let parent = &app.panels[0].entries[0];
        assert_eq!(parent.name, "..");
        assert!(parent.is_dir);
    }

    #[tokio::test]
    async fn refuses_to_delete_non_empty_directory() {
        let root = std::env::temp_dir().join(format!("mdir-del-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("full")).unwrap();
        fs::write(root.join("full/keep.txt"), b"x").unwrap();
        fs::create_dir_all(root.join("empty")).unwrap();
        let mut app = App::new();
        app.panels[0].location = Location::Local(root.clone());
        app.refresh(0).await.unwrap();

        let full = app.panels[0]
            .entries
            .iter()
            .position(|e| e.name == "full")
            .unwrap();
        app.panels[0].selected = full;
        app.delete_selected().await;
        assert!(root.join("full/keep.txt").exists());
        assert!(
            app.error
                .as_deref()
                .unwrap_or("")
                .contains("비어 있지 않은")
        );

        let empty = app.panels[0]
            .entries
            .iter()
            .position(|e| e.name == "empty")
            .unwrap();
        app.panels[0].selected = empty;
        app.delete_selected().await;
        assert!(!root.join("empty").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn root_does_not_have_parent_link() {
        let mut app = App::new();
        app.panels[0].location = Location::Local(PathBuf::from("/"));
        app.refresh(0).await.unwrap();
        assert!(app.panels[0].entries.iter().all(|entry| entry.name != ".."));
    }

    #[test]
    fn search_selects_case_insensitive_prefix() {
        let mut app = App::new();
        app.panels[0].entries = vec![
            Entry {
                name: "..".into(),
                path: "..".into(),
                is_dir: true,
                size: 0,
                modified: String::new(),
            },
            Entry {
                name: "Documents".into(),
                path: "Documents".into(),
                is_dir: true,
                size: 0,
                modified: String::new(),
            },
            Entry {
                name: "Workspace".into(),
                path: "Workspace".into(),
                is_dir: true,
                size: 0,
                modified: String::new(),
            },
        ];
        app.search = "wo".into();
        app.select_search_match();
        assert_eq!(app.panels[0].selected, 2);
    }

    #[tokio::test]
    async fn f2_toggles_remote_and_local_on_right_panel() {
        let mut app = App::new();
        let original = app.right_local_path.clone();
        app.toggle_remote_right().await.unwrap();
        assert!(matches!(app.panels[1].location, Location::Profiles));
        assert_eq!(app.active, 1);
        app.toggle_remote_right().await.unwrap();
        match &app.panels[1].location {
            Location::Local(path) => assert_eq!(path, &original),
            _ => panic!("right panel should return to local"),
        }
    }
}
