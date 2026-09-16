use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Row, Table, Wrap},
};

use crate::app::{App, Entry, Location, Mode, Panel};

// Mdir's VGA-like 16 colour palette.
const BLACK: Color = Color::Rgb(0, 0, 0);
const TEAL: Color = Color::Rgb(0, 170, 170);
const AQUA: Color = Color::Rgb(0, 255, 255);
const GREEN: Color = Color::Rgb(0, 255, 0);
const YELLOW: Color = Color::Rgb(255, 255, 85);
const ORANGE: Color = Color::Rgb(255, 85, 0);
const GRAY: Color = Color::Rgb(170, 170, 170);

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    f.render_widget(Block::default().style(Style::default().bg(BLACK)), area);

    let rows = Layout::vertical([
        Constraint::Length(1), // top function menu
        Constraint::Length(1), // path/volume bar
        Constraint::Min(5),    // shared file box
        Constraint::Length(1), // counts
        Constraint::Length(1), // status
        Constraint::Length(1), // bottom function menu
    ])
    .split(area);

    draw_top_menu(f, rows[0]);
    draw_path_bar(f, rows[1], app);
    draw_file_box(f, rows[2], app);
    draw_counts(f, rows[3], app);
    draw_status(f, rows[4], app);
    draw_bottom_menu(f, rows[5]);

    match app.mode {
        Mode::Help => help(f, area),
        Mode::ConfirmDelete => confirm(f, area, app),
        Mode::RemoteSetup => remote_setup(f, area, app),
        Mode::Normal => {}
    }
}

fn draw_top_menu(f: &mut Frame, area: Rect) {
    let items = [
        ("F1", "도움"),
        ("F2", "S3/R2"),
        ("F3", "보기"),
        ("F4", "편집"),
        ("F5", "복사"),
        ("F7", "폴더"),
        ("F8", "삭제"),
        ("F10", "종료"),
    ];
    let spans = items
        .into_iter()
        .flat_map(|(number, label)| {
            [
                Span::styled(number, Style::default().fg(YELLOW).bg(TEAL).bold()),
                Span::styled(
                    format!("{label:<7}"),
                    Style::default().fg(Color::White).bg(TEAL).bold(),
                ),
            ]
        })
        .collect::<Vec<_>>();
    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(TEAL)),
        area,
    );
}

fn draw_path_bar(f: &mut Frame, area: Rect, app: &App) {
    let path = location_name(&app.panels[app.active].location);
    let label = format!(" 경로 {path}");
    let right = if app.search.is_empty() {
        "볼륨명 [R-MDIR] ".to_string()
    } else {
        format!("검색: {} ", app.search)
    };
    let columns =
        Layout::horizontal([Constraint::Percentage(70), Constraint::Percentage(30)]).split(area);
    f.render_widget(
        Paragraph::new(label).style(Style::default().bg(GRAY).fg(BLACK).bold()),
        columns[0],
    );
    f.render_widget(
        Paragraph::new(right)
            .alignment(Alignment::Right)
            .style(Style::default().bg(GRAY).fg(BLACK).bold()),
        columns[1],
    );
}

fn draw_file_box(f: &mut Frame, area: Rect, app: &App) {
    let outer = Block::default()
        .borders(Borders::TOP | Borders::BOTTOM)
        .border_style(Style::default().fg(TEAL))
        .style(Style::default().bg(BLACK));
    let inner = outer.inner(area);
    f.render_widget(outer, area);

    let columns = Layout::horizontal([
        Constraint::Percentage(50),
        Constraint::Length(1),
        Constraint::Percentage(50),
    ])
    .split(inner);
    draw_panel(f, columns[0], &app.panels[0], app.active == 0);
    f.render_widget(
        Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(TEAL)),
        columns[1],
    );
    draw_panel(f, columns[2], &app.panels[1], app.active == 1);
}

fn draw_panel(f: &mut Frame, area: Rect, panel: &Panel, active: bool) {
    let rows = panel
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| entry_row(entry, active, active && index == panel.selected))
        .collect::<Vec<_>>();
    let widths = [
        Constraint::Min(9),
        Constraint::Length(6),
        Constraint::Length(11),
        Constraint::Length(14),
    ];
    let table = Table::new(rows, widths).column_spacing(1);
    let selected = active.then_some(panel.selected);
    let mut state = ratatui::widgets::TableState::default().with_selected(selected);
    f.render_stateful_widget(table, area.inner(Margin::new(1, 0)), &mut state);
}

fn entry_row(entry: &Entry, active: bool, selected: bool) -> Row<'static> {
    let (stem, ext) = display_name_parts(entry);
    let base_color = entry_color(entry, &ext);
    let color = if active {
        base_color
    } else {
        dim_color(base_color)
    };
    let style = if selected {
        Style::default()
            .fg(BLACK)
            .bg(color)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(color).bg(BLACK).bold()
    };
    Row::new(vec![
        stem,
        ext,
        if entry.is_dir {
            "<DIR>".into()
        } else {
            comma_size(entry.size)
        },
        entry.modified.clone(),
    ])
    .style(style)
}

fn dim_color(color: Color) -> Color {
    match color {
        Color::Rgb(red, green, blue) => Color::Rgb(
            (red as f32 * 0.8).round() as u8,
            (green as f32 * 0.8).round() as u8,
            (blue as f32 * 0.8).round() as u8,
        ),
        other => other,
    }
}

fn display_name_parts(entry: &Entry) -> (String, String) {
    if entry.is_dir {
        if entry.name == ".." {
            ("..".to_string(), "Up".to_string())
        } else {
            (entry.name.clone(), String::new())
        }
    } else if entry.name.starts_with('.') {
        // Unix dotfiles are names in their own right, not an empty stem plus extension.
        (entry.name.clone(), String::new())
    } else if let Some((stem, ext)) = entry.name.rsplit_once('.') {
        (stem.to_string(), ext.to_ascii_uppercase())
    } else {
        (entry.name.clone(), String::new())
    }
}

fn entry_color(entry: &Entry, ext: &str) -> Color {
    if entry.is_dir {
        ORANGE
    } else {
        match ext {
            "EXE" | "BAT" | "COM" | "APP" | "SH" => GREEN,
            "DOC" | "TXT" | "MD" | "JSON" | "TOML" | "YAML" | "YML" => TEAL,
            "ZIP" | "GZ" | "TAR" | "7Z" | "RAR" => Color::LightMagenta,
            "JPG" | "JPEG" | "PNG" | "GIF" | "WEBP" => AQUA,
            "RS" | "C" | "CPP" | "PY" | "JS" | "TS" => GREEN,
            _ => GRAY,
        }
    }
}

fn draw_counts(f: &mut Frame, area: Rect, app: &App) {
    let p = &app.panels[app.active];
    let files = p.entries.iter().filter(|e| !e.is_dir).count();
    let dirs = p
        .entries
        .iter()
        .filter(|e| e.is_dir && e.name != "..")
        .count();
    let bytes: u64 = p.entries.iter().filter(|e| !e.is_dir).map(|e| e.size).sum();
    let text = format!(
        " {files:>5} File   {dirs:>5} Dir       {:>14} Byte",
        comma_size(bytes)
    );
    f.render_widget(
        Paragraph::new(text).style(Style::default().bg(GRAY).fg(Color::White).bold()),
        area,
    );
}

fn draw_status(f: &mut Frame, area: Rect, app: &App) {
    if let Some(error) = &app.error {
        f.render_widget(
            Paragraph::new(format!(" {error}")).style(Style::default().bg(BLACK).fg(YELLOW).bold()),
            area,
        );
        return;
    }
    let entry = app.panels[app.active]
        .entries
        .get(app.panels[app.active].selected);
    let selected = entry
        .map(|e| format!("{}  {}  {}", e.name, comma_size(e.size), e.modified))
        .unwrap_or_else(|| app.status.clone());
    let foreground = entry
        .map(|e| {
            let (_, ext) = display_name_parts(e);
            entry_color(e, &ext)
        })
        .unwrap_or(GRAY);
    f.render_widget(
        Paragraph::new(format!(" {selected}"))
            .style(Style::default().bg(BLACK).fg(foreground).bold()),
        area,
    );
}

fn draw_bottom_menu(f: &mut Frame, area: Rect) {
    let now = chrono::Local::now();
    let columns =
        Layout::horizontal([Constraint::Percentage(48), Constraint::Percentage(52)]).split(area);
    let separator = || Span::styled("│", Style::default().fg(BLACK).bg(TEAL));
    let clock = Line::from(vec![
        separator(),
        footer_text(now.format("%y-%m-%d").to_string()),
        separator(),
        footer_text(now.format("%a").to_string().to_ascii_uppercase()),
        separator(),
        footer_text(now.format("%I:%M:%S%p").to_string().to_ascii_lowercase()),
        separator(),
    ]);
    f.render_widget(
        Paragraph::new(clock).style(Style::default().bg(TEAL)),
        columns[0],
    );
    let shortcuts = Line::from(vec![
        separator(),
        bottom_key("F5", "복사"),
        separator(),
        bottom_key("F7", "폴더"),
        separator(),
        bottom_key("F8", "삭제"),
        separator(),
        bottom_key("F10", "끝"),
        separator(),
    ]);
    f.render_widget(
        Paragraph::new(shortcuts)
            .alignment(Alignment::Right)
            .style(Style::default().bg(TEAL)),
        columns[1],
    );
}

fn bottom_key<'a>(key: &'a str, label: &'a str) -> Span<'a> {
    Span::styled(
        format!("{key}={label}"),
        Style::default().fg(YELLOW).bg(TEAL).bold(),
    )
}

fn footer_text(text: impl Into<String>) -> Span<'static> {
    Span::styled(
        format!(" {} ", text.into()),
        Style::default().fg(Color::White).bg(TEAL).bold(),
    )
}

fn location_name(loc: &Location) -> String {
    match loc {
        Location::Local(path) => path.display().to_string(),
        Location::Profiles => "S3/R2 원격 저장소".into(),
        Location::RemoteRoot { remote } => format!("{}://", remote.name),
        Location::Remote {
            remote,
            bucket,
            prefix,
        } => format!("{}://{bucket}/{prefix}", remote.name),
    }
}

fn comma_size(size: u64) -> String {
    let s = size.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height.min(area.height)),
        Constraint::Fill(1),
    ])
    .split(area);
    Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(width.min(area.width)),
        Constraint::Fill(1),
    ])
    .split(vertical[1])[1]
}

fn help(f: &mut Frame, area: Rect) {
    let popup = centered(area, 62, 20);
    f.render_widget(Clear, popup);
    let text = "키보드\n\n  영문/숫자       이름으로 빠른 이동\n  Backspace       검색어 지우기 / 상위 위치\n  ↑/↓             선택 이동\n  Enter/→         폴더 열기 · 실행 파일이면 실행 후 Pause\n  ←               상위 위치\n  Tab             활성 패널 전환\n  F2              S3/R2 프로필 목록\n  F5              반대편 로컬 패널로 복사/다운로드\n  F7              로컬 폴더 생성\n  F8              로컬 항목 삭제\n  Ctrl+R          새로 고침\n  Esc / q / F10   종료\n\n아무 키나 누르면 닫힙니다.";
    f.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" HELP ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(TEAL)),
            )
            .style(Style::default().bg(BLACK).fg(GRAY)),
        popup,
    );
}

fn remote_setup(f: &mut Frame, area: Rect, app: &App) {
    let popup = centered(area, 74, 22);
    f.render_widget(Clear, popup);
    let path = crate::config::config_path();
    let problem = match &app.config_error {
        Some(err) => format!("설정 파일을 읽지 못했습니다: {err}"),
        None if path.exists() => "설정 파일에 [remote.*] 항목이 없습니다.".into(),
        None => "설정 파일이 없습니다.".into(),
    };
    let text = format!(
        "S3/R2 프로필이 없습니다.\n\n{problem}\n설정 파일 경로: {}\n\n아래 내용을 위 경로에 저장하고 다시 실행하면 F2로 접속할 수 있습니다.\n\n  [remote.r2]\n  kind = \"r2\"\n  endpoint = \"https://<ACCOUNT_ID>.r2.cloudflarestorage.com\"\n  access_key_id = \"<R2 Access Key ID>\"\n  secret_access_key = \"<R2 Secret Access Key>\"\n  # bucket = \"버킷 하나만 보려면 지정\"\n\nAWS S3는 kind = \"s3\", region = \"ap-northeast-2\"만 적으면 ~/.aws 자격 증명을 씁니다.\n환경 변수 MDIR_CONFIG로 다른 설정 파일 경로를 지정할 수 있습니다.\n예시는 mdir.example.toml 참고.  아무 키나 누르면 닫힙니다.",
        path.display()
    );
    f.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" REMOTE SETUP ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(TEAL)),
            )
            .style(Style::default().bg(BLACK).fg(GRAY)),
        popup,
    );
}

fn confirm(f: &mut Frame, area: Rect, app: &App) {
    let name = app.panels[app.active]
        .entries
        .get(app.panels[app.active].selected)
        .map(|e| e.name.as_str())
        .unwrap_or("항목");
    let popup = centered(area, 52, 5);
    f.render_widget(Clear, popup);
    f.render_widget(
        Paragraph::new(format!("\n  '{name}'을(를) 삭제할까요?  [Y/N]"))
            .block(
                Block::default()
                    .title(" DELETE ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(TEAL)),
            )
            .style(Style::default().bg(BLACK).fg(YELLOW)),
        popup,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_comma_sizes() {
        assert_eq!(comma_size(999), "999");
        assert_eq!(comma_size(12_345_678), "12,345,678");
    }

    #[test]
    fn dotfile_is_not_treated_as_an_extension() {
        let entry = Entry {
            name: ".config.json".into(),
            path: ".config.json".into(),
            is_dir: false,
            size: 0,
            modified: String::new(),
            is_executable: false,
        };
        assert_eq!(
            display_name_parts(&entry),
            (".config.json".into(), String::new())
        );
    }

    #[test]
    fn inactive_color_is_dimmed_to_eighty_percent() {
        assert_eq!(dim_color(ORANGE), Color::Rgb(204, 68, 0));
        assert_eq!(dim_color(TEAL), Color::Rgb(0, 136, 136));
    }

    #[test]
    fn parent_link_is_displayed_as_dot_dot_up() {
        let entry = Entry {
            name: "..".into(),
            path: "..".into(),
            is_dir: true,
            size: 0,
            modified: String::new(),
            is_executable: false,
        };
        assert_eq!(display_name_parts(&entry), ("..".into(), "Up".into()));
    }
}
